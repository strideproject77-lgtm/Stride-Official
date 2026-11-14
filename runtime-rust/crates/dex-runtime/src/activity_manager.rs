//! Android Activity Manager and Task Backstack.
//!
//! Implements Android Activity task backstack, task affinity,
//! intent-based activity transitions, and proper lifecycle callbacks
//! (onCreate, onStart, onResume, onPause, onStop, onDestroy).

use crate::framework::{ActivityLifecycleState, Intent, Bundle};
use crate::loader::AppClassLoader;
use crate::interpreter::{ArtVm, ExecutionStopReason};
use crate::types::Value;

/// An active or paused Activity record in a task backstack.
#[derive(Debug, Clone)]
pub struct ActivityRecord {
    pub record_id: u32,
    pub class_name: String,
    pub intent: Intent,
    pub state: ActivityLifecycleState,
    pub saved_instance_state: Option<Bundle>,
    pub heap_object_ref: Option<u32>,
}

/// A task representing an ordered backstack of activities.
#[derive(Debug, Clone)]
pub struct ActivityTask {
    pub task_id: u32,
    pub stack: Vec<ActivityRecord>,
}

impl ActivityTask {
    pub fn new(task_id: u32) -> Self {
        Self {
            task_id,
            stack: Vec::new(),
        }
    }

    pub fn top(&self) -> Option<&ActivityRecord> {
        self.stack.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut ActivityRecord> {
        self.stack.last_mut()
    }
}

/// ActivityManager managing multiple tasks, the active activity stack, and transitions.
pub struct ActivityManager {
    tasks: Vec<ActivityTask>,
    current_task_index: usize,
    next_record_id: u32,
    next_task_id: u32,
}

impl Default for ActivityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityManager {
    pub fn new() -> Self {
        let mut initial_task = ActivityTask::new(1);
        Self {
            tasks: vec![initial_task],
            current_task_index: 0,
            next_record_id: 1,
            next_task_id: 2,
        }
    }

    /// Returns the currently active top activity, if any.
    pub fn current_activity(&self) -> Option<&ActivityRecord> {
        self.tasks.get(self.current_task_index).and_then(|t| t.top())
    }

    /// Returns total active activities in the current task.
    pub fn activity_count(&self) -> usize {
        self.tasks.get(self.current_task_index).map(|t| t.stack.len()).unwrap_or(0)
    }

    /// Starts a new activity, pausing/stopping the current activity and pushing the new one.
    pub fn start_activity(
        &mut self,
        intent: Intent,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> Result<u32, String> {
        let class_name = intent.component_class.clone()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| "Activity dispatch requires an explicit in-process component class".to_string())?;
        let record_id = self.next_record_id;
        let next_record_id = record_id.checked_add(1)
            .ok_or_else(|| "Activity record ID space exhausted".to_string())?;

        // Allocate before changing the current activity. A failed allocation must
        // leave the visible backstack and its lifecycle state untouched.
        let obj_ref = vm.heap.allocate_instance(&class_name)
            .map_err(|error| format!("Activity instance allocation failed for {}: {:?}", class_name, error))?;

        // 1. Pause and stop existing top activity if present
        if let Some(task) = self.tasks.get_mut(self.current_task_index) {
            if let Some(top) = task.top_mut() {
                if top.state == ActivityLifecycleState::Resumed {
                    top.state = ActivityLifecycleState::Paused;
                    Self::dispatch_method(&top.class_name, "onPause", "()V", top.heap_object_ref, class_loader, vm)?;
                    top.state = ActivityLifecycleState::Stopped;
                    Self::dispatch_method(&top.class_name, "onStop", "()V", top.heap_object_ref, class_loader, vm)?;
                }
            }
        }

        // 2. Construct the new Activity record only after allocation succeeds.
        let mut record = ActivityRecord {
            record_id,
            class_name: class_name.clone(),
            intent,
            state: ActivityLifecycleState::Created,
            saved_instance_state: None,
            heap_object_ref: Some(obj_ref),
        };

        // 3. Dispatch onCreate -> onStart -> onResume
        Self::dispatch_constructor(&class_name, obj_ref, class_loader, vm)?;
        Self::dispatch_create(&class_name, obj_ref, class_loader, vm)?;
        record.state = ActivityLifecycleState::Started;
        Self::dispatch_method(&class_name, "onStart", "()V", Some(obj_ref), class_loader, vm)?;
        record.state = ActivityLifecycleState::Resumed;
        Self::dispatch_method(&class_name, "onResume", "()V", Some(obj_ref), class_loader, vm)?;

        // 4. Push onto active task
        if let Some(task) = self.tasks.get_mut(self.current_task_index) {
            task.stack.push(record);
        }

        self.next_record_id = next_record_id;
        Ok(record_id)
    }

    /// Finishes the current top activity and resumes the previous one on the stack.
    /// Returns true if an activity was finished, false if the stack was already empty.
    pub fn finish_top_activity(
        &mut self,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> Result<bool, String> {
        let Some(original) = self.tasks.get(self.current_task_index)
            .and_then(|task| task.top()).cloned() else {
            return Ok(false);
        };

        // Keep the record on the stack until all of its finish callbacks return.
        // A guest exception or instruction-budget failure must not silently lose
        // the only Activity record that can be shown or diagnosed by the host.
        let finish_result = (|| {
            if original.state == ActivityLifecycleState::Resumed {
                if let Some(top) = self.tasks.get_mut(self.current_task_index).and_then(|task| task.top_mut()) {
                    top.state = ActivityLifecycleState::Paused;
                }
                Self::dispatch_method(&original.class_name, "onPause", "()V", original.heap_object_ref, class_loader, vm)?;
                if let Some(top) = self.tasks.get_mut(self.current_task_index).and_then(|task| task.top_mut()) {
                    top.state = ActivityLifecycleState::Stopped;
                }
                Self::dispatch_method(&original.class_name, "onStop", "()V", original.heap_object_ref, class_loader, vm)?;
            }
            if let Some(top) = self.tasks.get_mut(self.current_task_index).and_then(|task| task.top_mut()) {
                top.state = ActivityLifecycleState::Destroyed;
            }
            Self::dispatch_method(&original.class_name, "onDestroy", "()V", original.heap_object_ref, class_loader, vm)
        })();

        if let Err(error) = finish_result {
            // Restore the host-visible lifecycle state and retain the Activity.
            // Guest-side callback effects cannot be rolled back, but the task
            // remains recoverable and the original failure is preserved.
            if let Some(top) = self.tasks.get_mut(self.current_task_index).and_then(|task| task.top_mut()) {
                top.state = original.state;
            }
            return Err(error);
        }

        let current_task = self.tasks.get_mut(self.current_task_index)
            .ok_or_else(|| "Current Activity task disappeared during finish".to_owned())?;
        let finished = current_task.stack.pop()
            .ok_or_else(|| "Activity backstack changed during finish".to_owned())?;

        // Resume the previous Activity only after the finished record is safely
        // removed. If resume fails, preserve the remaining task and report it.
        if let Some(new_top) = current_task.top_mut() {
            new_top.state = ActivityLifecycleState::Started;
            Self::dispatch_method(&new_top.class_name, "onStart", "()V", new_top.heap_object_ref, class_loader, vm)?;
            new_top.state = ActivityLifecycleState::Resumed;
            Self::dispatch_method(&new_top.class_name, "onResume", "()V", new_top.heap_object_ref, class_loader, vm)?;
        }

        let _ = finished;
        Ok(true)
    }

    /// Handles Android hardware back button press.
    pub fn handle_back_pressed(
        &mut self,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> Result<bool, String> {
        self.finish_top_activity(class_loader, vm)
    }

    /// Runs the Activity constructor before lifecycle creation, matching Java object setup.
    fn dispatch_constructor(class_name: &str, receiver: u32, class_loader: &mut AppClassLoader, vm: &mut ArtVm) -> Result<(), String> {
        if let Some(method) = class_loader.find_method_in_hierarchy(class_name, "<init>", "()V") {
            let report = vm.execute_method(method, &[Value::Object(receiver)], 10_000)
                .map_err(|error| format!("Activity constructor interpreter error in {}: {:?}", class_name, error))?;
            Self::validate_lifecycle_report(class_name, "<init>", report.stop_reason)?;
        }
        Ok(())
    }

    /// Android Activity.onCreate receives the Activity receiver and nullable saved state.
    fn dispatch_create(class_name: &str, receiver: u32, class_loader: &mut AppClassLoader, vm: &mut ArtVm) -> Result<(), String> {
        if let Some(method) = class_loader.find_method_in_hierarchy(class_name, "onCreate", "(Landroid/os/Bundle;)V") {
            let report = vm.execute_method(method, &[Value::Object(receiver), Value::Null], 10_000)
                .map_err(|error| format!("Activity onCreate interpreter error in {}: {:?}", class_name, error))?;
            Self::validate_lifecycle_report(class_name, "onCreate", report.stop_reason)?;
        }
        Ok(())
    }

    fn dispatch_method(class_name: &str, method_name: &str, descriptor: &str, receiver: Option<u32>, class_loader: &mut AppClassLoader, vm: &mut ArtVm) -> Result<(), String> {
        if let Some(method) = class_loader.find_method_in_hierarchy(class_name, method_name, descriptor) {
            let mut args = Vec::new();
            if let Some(obj_ref) = receiver { args.push(Value::Object(obj_ref)); }
            let report = vm.execute_method(method, &args, 10_000)
                .map_err(|error| format!("Activity {} interpreter error in {}: {:?}", method_name, class_name, error))?;
            Self::validate_lifecycle_report(class_name, method_name, report.stop_reason)?;
        }
        Ok(())
    }

    fn validate_lifecycle_report(class_name: &str, method_name: &str, stop_reason: ExecutionStopReason) -> Result<(), String> {
        match stop_reason {
            ExecutionStopReason::UncaughtException(exception) => Err(format!("Activity {}.{} threw {}: {:?}", class_name, method_name, exception.class_descriptor, exception.message)),
            ExecutionStopReason::StepLimitExceeded => Err(format!("Activity {}.{} exceeded its lifecycle instruction budget", class_name, method_name)),
            ExecutionStopReason::ReturnedVoid | ExecutionStopReason::ReturnedValue(_) => Ok(()),
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_backstack_push_and_pop() {
        let mut am = ActivityManager::new();
        let mut class_loader = AppClassLoader::new();
        let mut vm_heap = crate::heap::ManagedHeap::default();
        let mut hierarchy = crate::hierarchy::ClassHierarchy::new();
        let mut monitors = crate::sync::MonitorTable::new();
        let mut jni = crate::jni::JniNativeRegistry::new();
        let methods = std::collections::HashMap::new();
        let strings: Vec<String> = Vec::new();
        let mut vm = ArtVm::new(&mut vm_heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);

        let mut intent1 = Intent::new(Some(Intent::ACTION_MAIN));
        intent1 = intent1.with_component("com.example.test", "MainActivity");
        let id1 = am.start_activity(intent1, &mut class_loader, &mut vm).unwrap();
        assert_eq!(id1, 1);
        assert_eq!(am.activity_count(), 1);
        assert_eq!(am.current_activity().unwrap().class_name, "MainActivity");
        assert_eq!(am.current_activity().unwrap().state, ActivityLifecycleState::Resumed);

        // Push second activity (e.g. SettingsActivity)
        let mut intent2 = Intent::new(Some(Intent::ACTION_VIEW));
        intent2 = intent2.with_component("com.example.test", "SettingsActivity");
        let id2 = am.start_activity(intent2, &mut class_loader, &mut vm).unwrap();
        assert_eq!(id2, 2);
        assert_eq!(am.activity_count(), 2);
        assert_eq!(am.current_activity().unwrap().class_name, "SettingsActivity");

        // Back pressed finishes SettingsActivity, restoring MainActivity to Resumed
        let handled = am.handle_back_pressed(&mut class_loader, &mut vm).unwrap();
        assert!(handled);
        assert_eq!(am.activity_count(), 1);
        assert_eq!(am.current_activity().unwrap().class_name, "MainActivity");
        assert_eq!(am.current_activity().unwrap().state, ActivityLifecycleState::Resumed);
    }
}
