//! Android Service Manager and Background Lifecycle.
//!
//! Manages Android Services (startService, stopService, foreground status,
//! and onCreate, onStartCommand, onDestroy lifecycle execution).

use std::collections::HashMap;
use crate::framework::Intent;
use crate::loader::AppClassLoader;
use crate::interpreter::ArtVm;
use crate::types::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Created,
    Started,
    Stopped,
    Destroyed,
}

/// Record representing a running or started Android Service.
#[derive(Debug, Clone)]
pub struct ServiceRecord {
    pub service_id: u32,
    pub class_name: String,
    pub is_foreground: bool,
    pub foreground_notification_id: Option<u32>,
    pub start_id: u32,
    pub state: ServiceState,
    pub heap_object_ref: Option<u32>,
}

/// ServiceManager managing all active Android background and foreground services.
pub struct ServiceManager {
    services: HashMap<String, ServiceRecord>,
    next_service_id: u32,
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            next_service_id: 1,
        }
    }

    pub fn running_services_count(&self) -> usize {
        self.services.len()
    }

    pub fn get_service(&self, class_name: &str) -> Option<&ServiceRecord> {
        self.services.get(class_name)
    }

    /// Starts a service via Intent, invoking onCreate (if newly started) and onStartCommand.
    pub fn start_service(
        &mut self,
        intent: &Intent,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> Result<u32, String> {
        let class_name = intent
            .component_class
            .clone()
            .unwrap_or_else(|| "UnknownService".to_string());

        let service_id = match self.services.get_mut(&class_name) {
            Some(existing) => {
                existing.start_id += 1;
                let sid = existing.start_id;
                let obj = existing.heap_object_ref;
                existing.state = ServiceState::Started;

                // Call onStartCommand(Intent, flags, startId)
                Self::dispatch_on_start_command(&class_name, obj, sid, heap, class_loader, vm);
                existing.service_id
            }
            None => {
                let sid = self.next_service_id;
                self.next_service_id += 1;

                // Allocate service instance
                let obj_ref = vm.heap.allocate_instance(&class_name).ok();

                // Call onCreate()
                Self::dispatch_method(&class_name, "onCreate", "()V", obj_ref, heap, class_loader, vm);

                // Call onStartCommand()
                Self::dispatch_on_start_command(&class_name, obj_ref, 1, heap, class_loader, vm);

                let record = ServiceRecord {
                    service_id: sid,
                    class_name: class_name.clone(),
                    is_foreground: false,
                    foreground_notification_id: None,
                    start_id: 1,
                    state: ServiceState::Started,
                    heap_object_ref: obj_ref,
                };
                self.services.insert(class_name, record);
                sid
            }
        };

        Ok(service_id)
    }

    /// Stops a running service, executing its onDestroy callback.
    pub fn stop_service(
        &mut self,
        class_name: &str,
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) -> bool {
        if let Some(mut record) = self.services.remove(class_name) {
            record.state = ServiceState::Destroyed;
            Self::dispatch_method(&record.class_name, "onDestroy", "()V", record.heap_object_ref, heap, class_loader, vm);
            true
        } else {
            false
        }
    }

    /// Promotes a running service to foreground state with a notification ID.
    pub fn start_foreground(&mut self, class_name: &str, notification_id: u32) -> bool {
        if let Some(record) = self.services.get_mut(class_name) {
            record.is_foreground = true;
            record.foreground_notification_id = Some(notification_id);
            true
        } else {
            false
        }
    }

    fn dispatch_on_start_command(
        class_name: &str,
        receiver: Option<u32>,
        start_id: u32,
        heap: &mut ManagedHeap,
        class_loader: &mut AppClassLoader,
        vm: &mut ArtVm,
    ) {
        if let Some(loaded_method) = class_loader.find_method(class_name, "onStartCommand", "(Landroid/content/Intent;II)I") {
            let mut args = Vec::new();
            if let Some(obj_ref) = receiver {
                args.push(Value::Object(obj_ref));
            }
            args.push(Value::Object(0)); // Null Intent ref for basic dispatch
            args.push(Value::Int(0));    // flags = 0
            args.push(Value::Int(start_id as i32));
            let _ = vm.execute_method(u16::try_from(loaded_method).unwrap_or(u16::MAX), &args, 10_000);
        }
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
            let _ = vm.execute_method(&loaded_method, &args, heap, 10_000);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_lifecycle_start_and_stop() {
        let mut sm = ServiceManager::new();
        let mut class_loader = AppClassLoader::new();
        let mut heap = crate::heap::ManagedHeap::default();
        let mut hierarchy = crate::hierarchy::ClassHierarchy::new();
        let mut monitors = crate::sync::MonitorTable::new();
        let mut jni = crate::jni::JniNativeRegistry::new();
        let methods = std::collections::HashMap::new();
        let strings: Vec<String> = Vec::new();
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);

        let mut intent = Intent::new(Some("com.example.DOWNLOAD"));
        intent = intent.with_component("com.example.test", "DownloadService");

        let sid = sm.start_service(&intent, &mut class_loader, &mut vm).unwrap();
        assert_eq!(sid, 1);
        assert_eq!(sm.running_services_count(), 1);

        let s = sm.get_service("DownloadService").unwrap();
        assert_eq!(s.state, ServiceState::Started);
        assert!(!s.is_foreground);

        // Promote to foreground
        sm.start_foreground("DownloadService", 1001);
        let s = sm.get_service("DownloadService").unwrap();
        assert!(s.is_foreground);
        assert_eq!(s.foreground_notification_id, Some(1001));

        // Stop service
        let stopped = sm.stop_service("DownloadService", &mut class_loader, &mut vm);
        assert!(stopped);
        assert_eq!(sm.running_services_count(), 0);
    }
}
