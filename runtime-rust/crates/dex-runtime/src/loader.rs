//! Multi-DEX Class Loading, Linking, and Class Initialization Engine.
//!
//! Handles class definitions across one or more DEX files, links class hierarchies,
//! resolves method/field references, and executes static initializers (<clinit>).

use std::collections::{HashMap, HashSet};
use stride_dex_inspector::classes::{ClassInventory, inspect_classes_for_loading};
use stride_dex_inspector::read_code_item;
use stride_dex_inspector::classes::validate_method_register_layout;
use stride_dex_inspector::validate_register_operands;
use crate::hierarchy::{ClassHierarchy, RuntimeClass};
use crate::interpreter::{MethodCode, ArtVm, ExecutionStopReason};
use crate::heap::ManagedHeap;
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;
use crate::types::Value;
use crate::diagnostics::{DiagnosticSeverity, RuntimeDiagnostics};

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
    pub interface_descriptors: Vec<String>,
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
    /// DEX-local identifier tables are selected by the method currently executing.
    pub method_dex_indices: HashMap<u32, usize>,
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
            method_dex_indices: HashMap::new(),
            string_pool: Vec::new(),
        }
    }

    /// Loads a DEX binary without a session recorder (useful for isolated loader tests).
    pub fn load_dex(&mut self, dex_bytes: &[u8]) -> Result<usize, String> {
        self.load_dex_internal(dex_bytes, None)
    }

    /// Loads a DEX binary and records parser, duplicate-definition and code-item faults.
    pub fn load_dex_with_diagnostics(
        &mut self,
        dex_bytes: &[u8],
        diagnostics: &RuntimeDiagnostics,
    ) -> Result<usize, String> {
        self.load_dex_internal(dex_bytes, Some(diagnostics))
    }

    fn load_dex_internal(
        &mut self,
        dex_bytes: &[u8],
        diagnostics: Option<&RuntimeDiagnostics>,
    ) -> Result<usize, String> {
        let inventory = match inspect_classes_for_loading(dex_bytes) {
            Ok(inventory) => inventory,
            Err(error) => {
                let detail = format!("DEX class inspection failed: {:?}", error);
                if let Some(diagnostics) = diagnostics {
                    diagnostics.record_detailed(
                        DiagnosticSeverity::RuntimeError, "DexLoader", "DEX_CLASS_INSPECTION_FAILED",
                        "DEX metadata inspection failed", Some(&detail),
                        Some("Use the reported method_id, code_item offset and DecodeError to identify the unsupported opcode, malformed branch/payload, or invalid register operand"),
                    );
                }
                return Err(detail);
            }
        };

        // Reject duplicate class definitions before mutating the loader, avoiding partial overwrite.
        let mut seen_classes = HashSet::new();
        for class_def in &inventory.classes {
            let descriptor = class_def.descriptor.as_str();
            if self.classes.contains_key(descriptor) || !seen_classes.insert(descriptor.to_owned()) {
                let detail = format!("Duplicate class definition across DEX inputs: {}", descriptor);
                if let Some(diagnostics) = diagnostics {
                    diagnostics.record_detailed(
                        DiagnosticSeverity::RuntimeError, "ClassLoader", "DUPLICATE_CLASS_DEFINITION",
                        "DEX contains a duplicate class definition", Some(&detail),
                        Some("Remove duplicate class definitions or inspect split/multidex packaging before loading"),
                    );
                }
                return Err(detail);
            }
        }

        let mut classes_loaded = 0;
        let dex_index = self.dex_inventories.len();

        for class_def in &inventory.classes {
            let descriptor = class_def.descriptor.clone();
            // DEX superclass_idx is a type_idx, not a class_def_idx.
            // Resolve against the owning DEX type table, including external parents.
            let super_descriptor = class_def.superclass_index
                .and_then(|idx| inventory.type_descriptors.get(idx as usize).cloned());

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
                let Some(next_method_id) = self.next_method_id.checked_add(1) else {
                    let detail = format!("Runtime method ID space exhausted while loading {}->{}{}", descriptor, m.name, m.prototype);
                    if let Some(diagnostics) = diagnostics {
                        diagnostics.record_detailed(
                            DiagnosticSeverity::FatalCrash, "ClassLoader", "METHOD_ID_SPACE_EXHAUSTED",
                            "Runtime method ID counter overflowed", Some(&detail),
                            Some("Stop loading this package; reduce method count or widen the runtime method-ID representation"),
                        );
                    }
                    return Err(detail);
                };
                self.next_method_id = next_method_id;
                let code = if m.code_offset != 0 {
                    match read_code_item(dex_bytes, m.code_offset) {
                        Ok(item) => {
                            let validation = validate_method_register_layout(
                                item.registers_size, item.ins_size, m.access_flags, &m.prototype,
                            ).map_err(|error| format!("register layout: {:?}", error))
                            .and_then(|_| validate_register_operands(item.instructions, item.registers_size)
                                .map_err(|error| format!("register operands: {:?}", error)));
                            if let Err(error) = validation {
                                if let Some(diagnostics) = diagnostics {
                                    diagnostics.record_detailed(
                                        DiagnosticSeverity::Warning, "DexLoader", "DEX_METHOD_REGISTER_LAYOUT_REJECTED",
                                        "Method metadata was retained but its code has invalid register usage",
                                        Some(&format!("{}->{}{} code_offset={} error={}",
                                            descriptor, m.name, m.prototype, m.code_offset, error)),
                                        Some("Check incoming argument word counts and every encoded register operand; this method will not execute"),
                                    );
                                }
                                None
                            } else {
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
                        }
                        Err(error) => {
                            if let Some(diagnostics) = diagnostics {
                                diagnostics.record_detailed(
                                    DiagnosticSeverity::Warning, "DexLoader", "DEX_CODE_ITEM_REJECTED",
                                    "Method code item could not be decoded; method metadata was retained without executable code",
                                    Some(&format!(
                                        "{}->{}{} code_offset={} error={:?}",
                                        descriptor, m.name, m.prototype, m.code_offset, error
                                    )),
                                    Some("Inspect code-item bounds, instruction length, try/catch data and DEX integrity; this method will not execute"),
                                );
                            }
                            None
                        },
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
                self.method_dex_indices.insert(mid, dex_index);
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
                interface_descriptors: class_def.interface_descriptors.clone(),
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
            rt_class.interface_descriptors = class.interface_descriptors.clone();
            rt_class.access_flags = class.access_flags;
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
    pub fn export_method_codes(&self) -> HashMap<u32, MethodCode> {
        let mut out = HashMap::new();
        for (method_id, loaded) in &self.methods_by_id {
            if let Some(code) = &loaded.code {
                out.insert(*method_id, code.clone());
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
        static_fields: &mut HashMap<(String, String), Value>,
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
        if let Some(ref superclass) = super_desc {
            if let Err(error) = self.initialize_class(superclass, heap, hierarchy, monitors, jni, static_fields, strings) {
                if let Some(class) = self.classes.get_mut(class_descriptor) {
                    class.init_state = ClassInitState::Error;
                }
                return Err(format!(
                    "Superclass initialization failed ({} -> {}): {}",
                    class_descriptor, superclass, error
                ));
            }
        }

        // Look for <clinit>()V
        let clinit_id = self.find_method(class_descriptor, "<clinit>", "()V");
        if let Some(mid) = clinit_id {
            let runtime_method_id = mid;
            let method_map = self.export_method_codes();
            let metadata = self.dex_inventories.first().cloned();
            let dex_strings = metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(strings);
            let method_lookup = self.method_lookup.clone();
            let dex_inventories = self.dex_inventories.clone();
            let method_dex_indices = self.method_dex_indices.clone();
            let mut vm = ArtVm::new(heap, hierarchy, monitors, jni, &method_map, dex_strings);
            vm.attach_multi_dex_metadata(&dex_inventories, &method_dex_indices);
            if let Some(inventory) = metadata.as_ref() {
                vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                vm.attach_method_resolver(&inventory.method_references, &method_lookup);
            }
            vm.attach_static_fields(static_fields);
            let report = vm.execute_method(runtime_method_id, &[], 50_000)?;
            match report.stop_reason {
                ExecutionStopReason::UncaughtException(ex) => {
                    if let Some(class) = self.classes.get_mut(class_descriptor) {
                        class.init_state = ClassInitState::Error;
                    }
                    return Err(format!("Class initialization failed for {}: {:?}", class_descriptor, ex));
                }
                ExecutionStopReason::StepLimitExceeded => {
                    if let Some(class) = self.classes.get_mut(class_descriptor) {
                        class.init_state = ClassInitState::Error;
                    }
                    return Err(format!("Class initializer exceeded 50000 instructions: {}", class_descriptor));
                }
                _ => {}
            }
        }

        if let Some(c) = self.classes.get_mut(class_descriptor) {
            c.init_state = ClassInitState::Initialized;
        }

        Ok(())
    }
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;

    #[test]
    fn dex_parse_failure_is_written_to_the_attached_session_report() {
        let mut loader = AppClassLoader::new();
        let diagnostics = RuntimeDiagnostics::new();
        let invalid_dex = [0u8; 8];
        let error = loader.load_dex_with_diagnostics(&invalid_dex, &diagnostics).unwrap_err();
        assert!(error.contains("DEX class inspection failed"));
        let report = diagnostics.export_text();
        assert!(report.contains("DEX_CLASS_INSPECTION_FAILED"), "{report}");
        assert!(report.contains("Recovery:"), "{report}");
    }
    #[test]
    fn loads_real_pocket_ridge_lite_dex_and_keeps_launcher_code() {
        let dex = include_bytes!("../tests/fixtures/pocket-ridge-lite-classes.dex");
        let mut loader = AppClassLoader::new();
        let diagnostics = RuntimeDiagnostics::new();

        let classes_loaded = loader
            .load_dex_with_diagnostics(dex, &diagnostics)
            .expect("real Pocket Ridge DEX should load");

        assert_eq!(classes_loaded, 2);
        assert!(loader.classes.contains_key("Lcom/stride/compatgame/MainActivity;"));
        let method_id = loader
            .find_method(
                "Lcom/stride/compatgame/MainActivity;",
                "onCreate",
                "(Landroid/os/Bundle;)V",
            )
            .expect("launcher onCreate method should be indexed");
        let method = loader.methods_by_id.get(&method_id).expect("method metadata should be retained");
        let code = method.code.as_ref().expect("launcher onCreate code item should decode");
        assert!(!code.code_units.is_empty());

        let report = diagnostics.export_text();
        assert!(!report.contains("DEX_CODE_ITEM_REJECTED"), "{report}");
    }

}
