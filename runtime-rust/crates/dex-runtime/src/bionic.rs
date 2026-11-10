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

        // --- libc memory ---
        self.register("malloc", |args| {
            let size = args.first().map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(if size > 0 { 0x1000 } else { 0 }))
        });
        self.register("calloc", |args| {
            let count = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            let size = args.get(1).map(|v| v.as_int()).unwrap_or(0);
            let _ = count.saturating_mul(size);
            Ok(Value::Int(0x1000))
        });
        self.register("realloc", |args| {
            let _ptr = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(0x1000))
        });
        self.register("free", |_args| Ok(Value::Int(0)));
        self.register("memset", |args| {
            let ptr = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(ptr))
        });
        self.register("memcpy", |args| {
            let dst = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(dst))
        });
        self.register("memmove", |args| {
            let dst = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(dst))
        });
        self.register("memcmp", |_args| Ok(Value::Int(0)));

        // --- libc strings ---
        self.register("strlen", |_args| Ok(Value::Int(0)));
        self.register("strcmp", |_args| Ok(Value::Int(0)));
        self.register("strncmp", |_args| Ok(Value::Int(0)));
        self.register("strcpy", |args| {
            let dst = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(dst))
        });
        self.register("strncpy", |args| {
            let dst = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(dst))
        });
        self.register("strcat", |args| {
            let dst = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(dst))
        });
        self.register("strchr", |_args| Ok(Value::Int(0)));
        self.register("strstr", |_args| Ok(Value::Int(0)));

        // --- libc stdio ---
        self.register("printf", |_args| Ok(Value::Int(0)));
        self.register("fprintf", |_args| Ok(Value::Int(0)));
        self.register("sprintf", |_args| Ok(Value::Int(0)));
        self.register("snprintf", |args| {
            let n = args.get(1).map(|v| v.as_int()).unwrap_or(0);
            Ok(Value::Int(n))
        });
        self.register("sscanf", |_args| Ok(Value::Int(0)));
        self.register("fopen", |_args| Ok(Value::Int(0)));
        self.register("fclose", |_args| Ok(Value::Int(0)));
        self.register("fread", |_args| Ok(Value::Int(0)));
        self.register("fwrite", |_args| Ok(Value::Int(0)));
        self.register("fseek", |_args| Ok(Value::Int(0)));
        self.register("ftell", |_args| Ok(Value::Int(0)));
        self.register("fflush", |_args| Ok(Value::Int(0)));

        // --- libm extras ---
        self.register("fabs", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.abs()))
        });
        self.register("fabsf", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.abs()))
        });
        self.register("fmod", |args| {
            let x = args.get(0).map(|v| v.as_double()).unwrap_or(0.0);
            let y = args.get(1).map(|v| v.as_double()).unwrap_or(1.0);
            Ok(Value::Double(x % y))
        });
        self.register("log", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(1.0);
            Ok(Value::Double(x.ln()))
        });
        self.register("log2", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(1.0);
            Ok(Value::Double(x.log2()))
        });
        self.register("log10", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(1.0);
            Ok(Value::Double(x.log10()))
        });
        self.register("exp", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.exp()))
        });
        self.register("asin", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.asin()))
        });
        self.register("acos", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.acos()))
        });
        self.register("atan", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.atan()))
        });
        self.register("round", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.round()))
        });
        self.register("roundf", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.round()))
        });
        self.register("sinf", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.sin()))
        });
        self.register("cosf", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.cos()))
        });
        self.register("sqrtf", |args| {
            let x = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Value::Double(x.sqrt()))
        });

        // --- libc time ---
        self.register("clock_gettime", |_args| Ok(Value::Int(0)));
        self.register("gettimeofday", |_args| Ok(Value::Int(0)));
        self.register("nanosleep", |_args| Ok(Value::Int(0)));
        self.register("usleep", |_args| Ok(Value::Int(0)));
        self.register("time", |_args| Ok(Value::Long(0)));

        // --- pthreads ---
        self.register("pthread_self", |_args| Ok(Value::Long(1)));
        self.register("pthread_create", |_args| Ok(Value::Int(0)));
        self.register("pthread_join", |_args| Ok(Value::Int(0)));
        self.register("pthread_detach", |_args| Ok(Value::Int(0)));
        self.register("pthread_mutex_init", |_args| Ok(Value::Int(0)));
        self.register("pthread_mutex_lock", |_args| Ok(Value::Int(0)));
        self.register("pthread_mutex_unlock", |_args| Ok(Value::Int(0)));
        self.register("pthread_mutex_destroy", |_args| Ok(Value::Int(0)));
        self.register("pthread_mutex_trylock", |_args| Ok(Value::Int(0)));
        self.register("pthread_cond_init", |_args| Ok(Value::Int(0)));
        self.register("pthread_cond_wait", |_args| Ok(Value::Int(0)));
        self.register("pthread_cond_signal", |_args| Ok(Value::Int(0)));
        self.register("pthread_cond_broadcast", |_args| Ok(Value::Int(0)));
        self.register("pthread_cond_destroy", |_args| Ok(Value::Int(0)));
        self.register("pthread_key_create", |_args| Ok(Value::Int(0)));
        self.register("pthread_getspecific", |_args| Ok(Value::Int(0)));
        self.register("pthread_setspecific", |_args| Ok(Value::Int(0)));
        self.register("pthread_once", |_args| Ok(Value::Int(0)));
        self.register("pthread_rwlock_init", |_args| Ok(Value::Int(0)));
        self.register("pthread_rwlock_rdlock", |_args| Ok(Value::Int(0)));
        self.register("pthread_rwlock_wrlock", |_args| Ok(Value::Int(0)));
        self.register("pthread_rwlock_unlock", |_args| Ok(Value::Int(0)));
        self.register("pthread_rwlock_destroy", |_args| Ok(Value::Int(0)));

        // --- libdl ---
        self.register("dlopen", |_args| Ok(Value::Int(1)));
        self.register("dlsym", |_args| Ok(Value::Int(0)));
        self.register("dlclose", |_args| Ok(Value::Int(0)));
        self.register("dlerror", |_args| Ok(Value::Int(0)));

        // --- libc misc ---
        self.register("exit", |_args| Err("Native library called exit()".into()));
        self.register("raise", |_args| Ok(Value::Int(0)));
        self.register("signal", |_args| Ok(Value::Int(0)));
        self.register("getpid", |_args| Ok(Value::Int(1234)));
        self.register("getenv", |_args| Ok(Value::Int(0)));
        self.register("setenv", |_args| Ok(Value::Int(0)));
        self.register("rand", |_args| Ok(Value::Int(42)));
        self.register("srand", |_args| Ok(Value::Int(0)));
        self.register("atoi", |_args| Ok(Value::Int(0)));
        self.register("atof", |_args| Ok(Value::Double(0.0)));
        self.register("strtol", |_args| Ok(Value::Long(0)));
        self.register("strtod", |_args| Ok(Value::Double(0.0)));

        // --- NDK system properties ---
        self.register("__system_property_get", |_args| Ok(Value::Int(0)));
        self.register("__system_property_find", |_args| Ok(Value::Int(0)));
        self.register("android_get_device_api_level", |_args| Ok(Value::Int(33)));

        // --- NDK AAssetManager ---
        self.register("AAssetManager_fromJava", |_args| Ok(Value::Int(1)));
        self.register("AAssetManager_open", |_args| Ok(Value::Int(0)));
        self.register("AAsset_read", |_args| Ok(Value::Int(0)));
        self.register("AAsset_seek", |_args| Ok(Value::Long(0)));
        self.register("AAsset_getLength", |_args| Ok(Value::Int(0)));
        self.register("AAsset_getLength64", |_args| Ok(Value::Long(0)));
        self.register("AAsset_close", |_args| Ok(Value::Int(0)));
        self.register("AAssetDir_open", |_args| Ok(Value::Int(0)));
        self.register("AAssetDir_getNextFileName", |_args| Ok(Value::Int(0)));
        self.register("AAssetDir_close", |_args| Ok(Value::Int(0)));

        // --- NDK OpenSL ES audio stubs ---
        self.register("slCreateEngine", |_args| Ok(Value::Int(0)));

        // --- abort ---
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

        let dynamic_info = inspect_elf_dynamic(elf_bytes, &elf_info)
            .map_err(|error| format!("ELF dynamic inspection failed for {}: {:?}", soname, error))?;

        for symbol in dynamic_info.symbols {
            if !symbol.name.is_empty() && symbol.section_index != 0 {
                if symbol.name == "JNI_OnLoad" {
                    has_jni_onload = true;
                }
                exported_symbols.insert(symbol.name, symbol.value);
            }
        }

        for (tag, value) in dynamic_info.dynamic_tags {
            if tag == 1 { // DT_NEEDED
                // The inspector currently exposes the string-table offset, not the decoded SONAME.
                needed_libraries.push(format!("DT_NEEDED string-table offset {}", value));
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
    #[test]
    fn rejects_truncated_native_library_without_registering_a_module() {
        let mut linker = NativeLinker::new();
        let error = linker.load_library("libtruncated.so", &[0u8; 8]).unwrap_err();
        assert!(error.contains("ELF inspection failed"), "{error}");
        assert!(linker.modules.is_empty());
    }

}
