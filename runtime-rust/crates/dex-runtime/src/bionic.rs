//! Android Bionic libc and native library dynamic linker emulation.
//!
//! Provides the symbol resolution table, Bionic runtime environment,
//! and native library relocation resolution for ELF ARM/ARM64 shared libraries.

use std::collections::HashMap;
use std::sync::Arc;
use stride_elf_inspector::{inspect_elf, inspect_elf_dynamic, ElfArchitecture};
use stride_native_loader::{plan_native_image, NativeImagePlan};
use crate::types::Value;
use crate::jni::JniEnv;

pub type BionicFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Bionic libc/libm/liblog/libdl symbol emulation environment.
#[derive(Default, Clone)]
pub struct BionicEnvironment {
    symbols: HashMap<String, BionicFn>,
}

impl BionicEnvironment {
    pub fn new() -> Self {
        let mut env = Self::default();
        env.register_core_symbols();
        env
    }

    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&[Value]) -> Result<Value, String> + Send + Sync + 'static,
    {
        self.symbols.insert(name.to_string(), Arc::new(handler));
    }

    pub fn resolve_symbol(&self, name: &str) -> Option<&BionicFn> {
        self.symbols.get(name)
    }

    fn register_core_symbols(&mut self) {
        // --- liblog ---
        self.register("__android_log_print", |args| {
            // prio, tag, fmt, ...
            let prio = args.get(0).map(|v| v.as_int()).unwrap_or(4); // INFO
            Ok(Value::Int(prio))
        });

        self.register("__android_log_write", |args| {
            let prio = args.get(0).map(|v| v.as_int()).unwrap_or(4);
            Ok(Value::Int(prio))
        });

        // --- libm ---
        self.register("sin", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.sin()))
        });

        self.register("cos", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.cos()))
        });

        self.register("tan", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.tan()))
        });

        self.register("sqrt", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.sqrt()))
        });

        self.register("atan2", |args| {
            let y = args.get(0).map(|v| v.as_double()).unwrap_or(0.0);
            let x = args.get(1).map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(y.atan2(x)))
        });

        self.register("pow", |args| {
            let base = args.get(0).map(|v| v.as_double()).unwrap_or(0.0);
            let exp = args.get(1).map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(base.powf(exp)))
        });

        self.register("floor", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.floor()))
        });

        self.register("ceil", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.ceil()))
        });

        // --- libc memory & time ---
        self.register("strlen", |_args| {
            Ok(Value::Int(0))
        });

        self.register("abort", |_args| {
            Err("Native library called Bionic abort()".into())
        });
    }
}

/// A loaded Android shared library module.
#[derive(Debug, Clone)]
pub struct LoadedNativeModule {
    pub name: String,
    pub architecture: ElfArchitecture,
    pub plan: NativeImagePlan,
    pub exported_symbols: HashMap<String, u64>,
    pub needed_libraries: Vec<String>,
    pub has_jni_onload: bool,
}

/// Native shared library linker coordinating ELF loading and symbol resolution.
pub struct NativeLinker {
    pub bionic: BionicEnvironment,
    pub modules: HashMap<String, LoadedNativeModule>,
    pub loaded_order: Vec<String>,
}

impl Default for NativeLinker {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeLinker {
    pub fn new() -> Self {
        Self {
            bionic: BionicEnvironment::new(),
            modules: HashMap::new(),
            loaded_order: Vec::new(),
        }
    }

    /// Inspects and links an ELF shared library binary into the native environment.
    pub fn load_library(&mut self, soname: &str, elf_bytes: &[u8]) -> Result<&LoadedNativeModule, String> {
        let elf_info = inspect_elf(elf_bytes)
            .map_err(|e| format!("ELF inspection failed for {}: {:?}", soname, e))?;

        let plan = plan_native_image(&elf_info, 0)
            .map_err(|e| format!("Native image planning failed for {}: {:?}", soname, e))?;

        let mut exported_symbols = HashMap::new();
        let mut needed_libraries = Vec::new();
        let mut has_jni_onload = false;

        if let Ok(dyn_info) = inspect_elf_dynamic(elf_bytes, &elf_info) {
            for sym in dyn_info.symbols {
                if !sym.name.is_empty() && sym.section_index != 0 {
                    if sym.name == "JNI_OnLoad" {
                        has_jni_onload = true;
                    }
                    exported_symbols.insert(sym.name, sym.value);
                }
            }

            for (tag, val) in dyn_info.dynamic_tags {
                if tag == 1 { // DT_NEEDED
                    // val is offset into string table
                    needed_libraries.push(format!("lib_{}", val));
                }
            }
        }

        let module = LoadedNativeModule {
            name: soname.to_string(),
            architecture: plan.architecture,
            plan,
            exported_symbols,
            needed_libraries,
            has_jni_onload,
        };

        self.modules.insert(soname.to_string(), module);
        if !self.loaded_order.contains(&soname.to_string()) {
            self.loaded_order.push(soname.to_string());
        }

        Ok(self.modules.get(soname).unwrap())
    }

    /// Resolves a symbol across all loaded native modules and Bionic environment.
    pub fn resolve_symbol(&self, name: &str) -> Option<ResolvedSymbol<'_>> {
        for soname in self.loaded_order.iter().rev() {
            if let Some(m) = self.modules.get(soname) {
                if let Some(&addr) = m.exported_symbols.get(name) {
                    return Some(ResolvedSymbol::NativeFunction {
                        soname: &m.name,
                        address: addr,
                    });
                }
            }
        }

        if let Some(bionic_fn) = self.bionic.resolve_symbol(name) {
            return Some(ResolvedSymbol::BionicBuiltin(bionic_fn));
        }

        None
    }

    /// Finds JNI native method binding for a Java native method.
    pub fn find_jni_binding(&self, class_descriptor: &str, method_name: &str) -> Option<ResolvedSymbol<'_>> {
        let clean_class = class_descriptor.trim_start_matches('L').trim_end_matches(';').replace('/', "_");
        let candidate = format!("Java_{}_{}", clean_class, method_name);
        self.resolve_symbol(&candidate)
    }
}

pub enum ResolvedSymbol<'a> {
    NativeFunction { soname: &'a str, address: u64 },
    BionicBuiltin(&'a BionicFn),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bionic_evaluates_math_and_logging() {
        let bionic = BionicEnvironment::new();
        let sin_fn = bionic.resolve_symbol("sin").unwrap();
        let res = sin_fn(&[Value::Double(0.0)]).unwrap();
        assert_eq!(res, Value::Double(0.0));

        let log_fn = bionic.resolve_symbol("__android_log_print").unwrap();
        let log_res = log_fn(&[Value::Int(3)]).unwrap();
        assert_eq!(log_res, Value::Int(3));
    }

    #[test]
    fn native_linker_resolves_jni_binding_name() {
        let mut linker = NativeLinker::new();
        let mut syms = HashMap::new();
        syms.insert("Java_com_example_app_NativeLib_stringFromJNI".to_string(), 0x1000);

        let mod_plan = NativeImagePlan {
            load_bias: 0,
            image_virtual_start: 0,
            image_size: 4096,
            pages: Vec::new(),
            segment_copies: Vec::new(),
            architecture: ElfArchitecture::Aarch64,
        };

        linker.modules.insert(
            "libnative-lib.so".to_string(),
            LoadedNativeModule {
                name: "libnative-lib.so".to_string(),
                architecture: ElfArchitecture::Aarch64,
                plan: mod_plan,
                exported_symbols: syms,
                needed_libraries: Vec::new(),
                has_jni_onload: true,
            },
        );
        linker.loaded_order.push("libnative-lib.so".to_string());

        let resolved = linker.find_jni_binding("Lcom/example/app/NativeLib;", "stringFromJNI");
        assert!(matches!(resolved, Some(ResolvedSymbol::NativeFunction { soname: "libnative-lib.so", address: 0x1000 })));
    }
}
