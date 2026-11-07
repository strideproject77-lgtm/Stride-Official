//! Multi-DEX Class Loading, Linking, and Class Initialization Engine.
//!
//! Handles class definitions across one or more DEX files, links class hierarchies,
//! resolves method/field references, and executes static initializers (<clinit>).

use std::collections::HashMap;
use stride_dex_inspector::classes::{ClassInventory, inspect_classes};
use stride_dex_inspector::read_code_item;
use crate::hierarchy::{ClassHierarchy, RuntimeClass};
use crate::interpreter::{MethodCode, ArtVm, ExecutionStopReason};
use crate::heap::ManagedHeap;
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;
use crate::types::Value;

/// Initialization state of a class in the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassInitState {
    NotInitialized,
    Initializing,
    Initialized,
    Error,
}

/// A loaded method in the Multi-DEX class loader.
#[derive(Debug, Clone)]
pub struct LoadedMethod {
    pub method_id: u32,
    pub class_descriptor: String,
    pub name: String,
    pub prototype: String,
    pub access_flags: u32,
    pub is_direct: bool,
    pub code: Option<MethodCode>,
}

/// A loaded class in the Multi-DEX class loader.
#[derive(Debug, Clone)]
pub struct LoadedClass {
    pub descriptor: String,
    pub super_descriptor: Option<String>,
    pub access_flags: u32,
    pub init_state: ClassInitState,
    pub static_fields: HashMap<String, Value>,
    pub instance_field_names: Vec<String>,
    pub direct_methods: Vec<u32>,
    pub virtual_methods: Vec<u32>,
}

/// Multi-DEX Class Loader that manages loaded classes, methods, and hierarchies.
pub struct AppClassLoader {
    pub classes: HashMap<String, LoadedClass>,
    pub methods_by_id: HashMap<u32, LoadedMethod>,
    pub method_lookup: HashMap<(String, String, String), u32>,
    pub next_method_id: u32,
    pub dex_inventories: Vec<ClassInventory>,
    pub string_pool: Vec<String>,
}

impl AppClassLoader {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            methods_by_id: HashMap::new(),
            method_lookup: HashMap::new(),
            next_method_id: 1,
            dex_inventories: Vec::new(),
            string_pool: Vec::new(),
        }
    }

    /// Loads a DEX binary into the class loader, indexing classes and method definitions.
    pub fn load_dex(&mut self, dex_bytes: &[u8]) -> Result<usize, String> {
        let inventory = inspect_classes(dex_bytes)
            .map_err(|e| format!("DEX class inspection failed: {:?}", e))?;

        let mut classes_loaded = 0;

        for class_def in &inventory.classes {
            let descriptor = class_def.descriptor.clone();
            let super_descriptor = class_def.superclass_index.and_then(|idx| {
                inventory.classes.iter().find(|c| c.class_index == idx).map(|c| c.descriptor.clone())
            });

            let mut static_fields = HashMap::new();
            let mut instance_field_names = Vec::new();

            for field in &class_def.fields {
                if field.is_static {
                    static_fields.insert(field.name.clone(), Value::Int(0));
                } else {
                    instance_field_names.push(field.name.clone());
                }
            }

            let mut direct_methods = Vec::new();
            let mut virtual_methods = Vec::new();

            for m in &class_def.methods {
                let mid = self.next_method_id;
                self.next_method_id += 1;

                let code = if m.code_offset != 0 {
                    match read_code_item(dex_bytes, m.code_offset) {
                        Ok(item) => {
                            let mut units = Vec::with_capacity(item.instructions.len() / 2);
                            for chunk in item.instructions.chunks_exact(2) {
                                units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
                            }
                            Some(MethodCode {
                                class_descriptor: descriptor.clone(),
                                name: m.name.clone(),
                                prototype: m.prototype.clone(),
                                code_units: units,
                                registers_size: item.registers_size as usize,
                                ins_size: item.ins_size as usize,
                                try_blocks: Vec::new(),
                            })
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };

                let loaded_m = LoadedMethod {
                    method_id: mid,
                    class_descriptor: descriptor.clone(),
                    name: m.name.clone(),
                    prototype: m.prototype.clone(),
                    access_flags: m.access_flags,
                    is_direct: m.is_direct,
                    code,
                };

                self.methods_by_id.insert(mid, loaded_m);
                self.method_lookup.insert(
                    (descriptor.clone(), m.name.clone(), m.prototype.clone()),
                    mid,
                );

                if m.is_direct {
                    direct_methods.push(mid);
                } else {
                    virtual_methods.push(mid);
                }
            }

            let loaded_class = LoadedClass {
                descriptor: descriptor.clone(),
                super_descriptor,
                access_flags: class_def.access_flags,
                init_state: ClassInitState::NotInitialized,
                static_fields,
                instance_field_names,
                direct_methods,
                virtual_methods,
            };

            self.classes.insert(descriptor, loaded_class);
            classes_loaded += 1;
        }

        self.dex_inventories.push(inventory);
        Ok(classes_loaded)
    }

    /// Links all loaded classes into the given ClassHierarchy.
    pub fn link_hierarchy(&self, hierarchy: &mut ClassHierarchy) {
        for (desc, class) in &self.classes {
            let super_name = class.super_descriptor.as_deref().or(Some("Ljava/lang/Object;"));
            let mut rt_class = RuntimeClass::new(desc, super_name);
            for &mid in &class.virtual_methods {
                if let Some(m) = self.methods_by_id.get(&mid) {
                    rt_class.register_method(&m.name, &m.prototype, mid, false);
                }
            }
            for &mid in &class.direct_methods {
                if let Some(m) = self.methods_by_id.get(&mid) {
                    rt_class.register_method(&m.name, &m.prototype, mid, true);
                }
            }
            hierarchy.register_class(rt_class);
        }
    }

    /// Finds a method ID by class descriptor, method name, and prototype.
    pub fn find_method(&self, class: &str, name: &str, proto: &str) -> Option<u32> {
        self.method_lookup.get(&(class.to_string(), name.to_string(), proto.to_string())).copied()
    }

    /// Converts loaded methods into the format consumed by ArtVm.
    pub fn export_method_codes(&self) -> HashMap<u16, MethodCode> {
        let mut out = HashMap::new();
        for (mid, loaded) in &self.methods_by_id {
            if let Some(code) = &loaded.code {
                if let Ok(id_u16) = u16::try_from(*mid) {
                    out.insert(id_u16, code.clone());
                }
            }
        }
        out
    }

    /// Runs `<clinit>` static class initializers for a class if not already initialized.
    pub fn initialize_class(
        &mut self,
        class_descriptor: &str,
        heap: &mut ManagedHeap,
        hierarchy: &mut ClassHierarchy,
        monitors: &mut MonitorTable,
        jni: &mut JniNativeRegistry,
        strings: &[String],
    ) -> Result<(), String> {
        let state = self.classes.get(class_descriptor).map(|c| c.init_state);
        match state {
            Some(ClassInitState::Initialized) | Some(ClassInitState::Initializing) => return Ok(()),
            None => return Ok(()), // External / framework class
            _ => {}
        }

        if let Some(c) = self.classes.get_mut(class_descriptor) {
            c.init_state = ClassInitState::Initializing;
        }

        // Initialize superclass first
        let super_desc = self.classes.get(class_descriptor).and_then(|c| c.super_descriptor.clone());
        if let Some(ref s) = super_desc {
            let _ = self.initialize_class(s, heap, hierarchy, monitors, jni, strings);
        }

        // Look for <clinit>()V
        let clinit_id = self.find_method(class_descriptor, "<clinit>", "()V");
        if let Some(mid) = clinit_id {
            if let Ok(id16) = u16::try_from(mid) {
                let method_map = self.export_method_codes();
                let mut vm = ArtVm::new(heap, hierarchy, monitors, jni, &method_map, strings);
                let report = vm.execute_method(id16, &[], 50_000)?;
                if let ExecutionStopReason::UncaughtException(ex) = report.stop_reason {
                    if let Some(c) = self.classes.get_mut(class_descriptor) {
                        c.init_state = ClassInitState::Error;
                    }
                    return Err(format!("Class initialization failed for {}: {:?}", class_descriptor, ex));
                }
            }
        }

        if let Some(c) = self.classes.get_mut(class_descriptor) {
            c.init_state = ClassInitState::Initialized;
        }

        Ok(())
    }
}
