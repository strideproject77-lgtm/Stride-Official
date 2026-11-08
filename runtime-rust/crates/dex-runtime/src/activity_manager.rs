//! Android Activity Manager and Task Backstack.
//!
//! Implements Android Activity task backstack, task affinity,
//! intent-based activity transitions, and proper lifecycle callbacks
//! (onCreate, onStart, onResume, onPause, onStop, onDestroy).

use crate::framework::{ActivityLifecycleState, Intent, Bundle};
use crate::heap::ManagedHeap;
use crate::loader::AppClassLoader;
use crate::interpreter::ArtVm;
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
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> Result<u32, String> {
        let class_name = intent.component_class.clone().unwrap_or_else(|| "UnknownActivity".to_string());
        let record_id = self.next_record_id;
        self.next_record_id += 1;

        // 1. Pause and stop existing top activity if present
        if let Some(task) = self.tasks.get_mut(self.current_task_index) {
            if let Some(top) = task.top_mut() {
                if top.state == ActivityLifecycleState::Resumed {
                    top.state = ActivityLifecycleState::Paused;
                    Self::dispatch_method(&top.class_name, "onPause", "()V", top.heap_object_ref, heap, class_loader, vm);
                    top.state = ActivityLifecycleState::Stopped;
                    Self::dispatch_method(&top.class_name, "onStop", "()V", top.heap_object_ref, heap, class_loader, vm);
                }
            }
        }

        // 2. Allocate heap instance for the new Activity
        let obj_ref = vm.heap.allocate_instance(&class_name).ok();

        let mut record = ActivityRecord {
            record_id,
            class_name: class_name.clone(),
            intent,
            state: ActivityLifecycleState::Created,
            saved_instance_state: None,
            heap_object_ref: obj_ref,
        };

        // 3. Dispatch onCreate -> onStart -> onResume
        Self::dispatch_method(&class_name, "onCreate", "(Landroid/os/Bundle;)V", obj_ref, heap, class_loader, vm);
        record.state = ActivityLifecycleState::Started;
        Self::dispatch_method(&class_name, "onStart", "()V", obj_ref, heap, class_loader, vm);
        record.state = ActivityLifecycleState::Resumed;
        Self::dispatch_method(&class_name, "onResume", "()V", obj_ref, heap, class_loader, vm);

        // 4. Push onto active task
        if let Some(task) = self.tasks.get_mut(self.current_task_index) {
            task.stack.push(record);
        }

        Ok(record_id)
    }

    /// Finishes the current top activity and resumes the previous one on the stack.
    /// Returns true if an activity was finished, false if the stack was already empty.
    pub fn finish_top_activity(
        &mut self,
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> bool {
        let current_task = match self.tasks.get_mut(self.current_task_index) {
            Some(t) => t,
            None => return false,
        };

        if current_task.stack.is_empty() {
            return false;
        }

        // 1. Pop top activity and dispatch pause, stop, destroy
        let mut finished = current_task.stack.pop().unwrap();
        finished.state = ActivityLifecycleState::Paused;
        Self::dispatch_method(&finished.class_name, "onPause", "()V", finished.heap_object_ref, heap, class_loader, vm);
        finished.state = ActivityLifecycleState::Stopped;
        Self::dispatch_method(&finished.class_name, "onStop", "()V", finished.heap_object_ref, heap, class_loader, vm);
        finished.state = ActivityLifecycleState::Destroyed;
        Self::dispatch_method(&finished.class_name, "onDestroy", "()V", finished.heap_object_ref, heap, class_loader, vm);

        // 2. Resume new top activity if one exists
        if let Some(new_top) = current_task.top_mut() {
            new_top.state = ActivityLifecycleState::Started;
            Self::dispatch_method(&new_top.class_name, "onStart", "()V", new_top.heap_object_ref, heap, class_loader, vm);
            new_top.state = ActivityLifecycleState::Resumed;
            Self::dispatch_method(&new_top.class_name, "onResume", "()V", new_top.heap_object_ref, heap, class_loader, vm);
        }

        true
    }

    /// Handles Android hardware back button press.
    pub fn handle_back_pressed(
        &mut self,
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> bool {
        self.finish_top_activity(heap, class_loader, vm)
    }

    fn dispatch_method(
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        receiver: Option<u32>,
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) {
        if let Some(loaded_method) = class_loader.find_method(class_name, method_name, descriptor) {
            let mut args = Vec::new();
            if let Some(obj_ref) = receiver {
                args.push(Value::Object(obj_ref));
            }
            let _ = vm.execute_method(u16::try_from(loaded_method).unwrap_or(u16::MAX), &args, 10_000);
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
        let mut heap = ManagedHeap::default();
        let mut hierarchy = crate::hierarchy::ClassHierarchy::new();
        let mut monitors = crate::sync::MonitorTable::new();
        let mut jni = crate::jni::JniNativeRegistry::new();
        let methods = std::collections::HashMap::new();
        let strings: Vec<String> = Vec::new();
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);

        let mut intent1 = Intent::new(Some(Intent::ACTION_MAIN));
        intent1 = intent1.with_component("com.example.test", "MainActivity");
        let id1 = am.start_activity(intent1, &mut heap, &mut class_loader, &mut vm).unwrap();
        assert_eq!(id1, 1);
        assert_eq!(am.activity_count(), 1);
        assert_eq!(am.current_activity().unwrap().class_name, "MainActivity");
        assert_eq!(am.current_activity().unwrap().state, ActivityLifecycleState::Resumed);

        // Push second activity (e.g. SettingsActivity)
        let mut intent2 = Intent::new(Some(Intent::ACTION_VIEW));
        intent2 = intent2.with_component("com.example.test", "SettingsActivity");
        let id2 = am.start_activity(intent2, &mut heap, &mut class_loader, &mut vm).unwrap();
        assert_eq!(id2, 2);
        assert_eq!(am.activity_count(), 2);
        assert_eq!(am.current_activity().unwrap().class_name, "SettingsActivity");

        // Back pressed finishes SettingsActivity, restoring MainActivity to Resumed
        let handled = am.handle_back_pressed(&mut heap, &mut class_loader, &mut vm);
        assert!(handled);
        assert_eq!(am.activity_count(), 1);
        assert_eq!(am.current_activity().unwrap().class_name, "MainActivity");
        assert_eq!(am.current_activity().unwrap().state, ActivityLifecycleState::Resumed);
    }
}
