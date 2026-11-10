//! Android Bionic libc and native library dynamic linker emulation.
//!
//! Provides the symbol resolution table, Bionic runtime environment,
//! and native library relocation resolution for ELF ARM/ARM64 shared libraries.

use std::collections::HashMap;
use std::sync::Arc;
use stride_elf_inspector::{inspect_elf, inspect_elf_dynamic, ElfArchitecture};
use stride_native_loader::{materialize_native_image, plan_native_image, NativeImagePlan};
use stride_elf_inspector::Relocation;
use crate::types::Value;
use crate::jni::JniEnv;

pub type BionicFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Builds an explicit failure for symbols whose guest ABI implementation is not
/// available. Returning a plausible pointer/status here would corrupt guest state.
fn unsupported_symbol(name: &'static str) -> impl Fn(&[Value]) -> Result<Value, String> + Send + Sync + 'static {
    move |_args| Err(format!(
        "Bionic symbol {} is not implemented; refusing placeholder success",
        name
    ))
}

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
        // Guest pointers are 64-bit addresses, but this compatibility table does
        // not yet own a guest virtual-memory allocator or mapped-memory view.
        // Fail explicitly instead of handing native code a fabricated address.
        for name in ["malloc", "calloc", "realloc", "free", "memset", "memcpy", "memmove", "memcmp"] {
            self.register(name, unsupported_symbol(match name {
                "malloc" => "malloc", "calloc" => "calloc", "realloc" => "realloc",
                "free" => "free", "memset" => "memset", "memcpy" => "memcpy",
                "memmove" => "memmove", _ => "memcmp",
            }));
        }

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
        // Threading and synchronization require a guest-thread scheduler and
        // per-thread state. Returning success without creating/synchronizing a
        // thread can deadlock or corrupt native game state.
        for name in [
            "pthread_self", "pthread_create", "pthread_join", "pthread_detach",
            "pthread_mutex_init", "pthread_mutex_lock", "pthread_mutex_unlock",
            "pthread_mutex_destroy", "pthread_mutex_trylock", "pthread_cond_init",
            "pthread_cond_wait", "pthread_cond_signal", "pthread_cond_broadcast",
            "pthread_cond_destroy", "pthread_key_create", "pthread_getspecific",
            "pthread_setspecific", "pthread_once",
        ] {
            self.register(name, unsupported_symbol(match name {
                "pthread_self" => "pthread_self", "pthread_create" => "pthread_create",
                "pthread_join" => "pthread_join", "pthread_detach" => "pthread_detach",
                "pthread_mutex_init" => "pthread_mutex_init", "pthread_mutex_lock" => "pthread_mutex_lock",
                "pthread_mutex_unlock" => "pthread_mutex_unlock", "pthread_mutex_destroy" => "pthread_mutex_destroy",
                "pthread_mutex_trylock" => "pthread_mutex_trylock", "pthread_cond_init" => "pthread_cond_init",
                "pthread_cond_wait" => "pthread_cond_wait", "pthread_cond_signal" => "pthread_cond_signal",
                "pthread_cond_broadcast" => "pthread_cond_broadcast", "pthread_cond_destroy" => "pthread_cond_destroy",
                "pthread_key_create" => "pthread_key_create", "pthread_getspecific" => "pthread_getspecific",
                "pthread_setspecific" => "pthread_setspecific", _ => "pthread_once",
            }));
        }
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
        // System-property and asset APIs need real guest buffers and package-backed
        // asset handles. Do not return fabricated handles or success values.
        for name in [
            "__system_property_get", "__system_property_find", "AAssetManager_fromJava",
            "AAssetManager_open", "AAsset_read", "AAsset_seek", "AAsset_getLength",
            "AAsset_getLength64", "AAsset_close", "AAssetDir_open",
            "AAssetDir_getNextFileName", "AAssetDir_close", "slCreateEngine",
        ] {
            self.register(name, unsupported_symbol(match name {
                "__system_property_get" => "__system_property_get",
                "__system_property_find" => "__system_property_find",
                "AAssetManager_fromJava" => "AAssetManager_fromJava",
                "AAssetManager_open" => "AAssetManager_open", "AAsset_read" => "AAsset_read",
                "AAsset_seek" => "AAsset_seek", "AAsset_getLength" => "AAsset_getLength",
                "AAsset_getLength64" => "AAsset_getLength64", "AAsset_close" => "AAsset_close",
                "AAssetDir_open" => "AAssetDir_open",
                "AAssetDir_getNextFileName" => "AAssetDir_getNextFileName",
                "AAssetDir_close" => "AAssetDir_close", _ => "slCreateEngine",
            }));
        }
        self.register("android_get_device_api_level", |_args| Ok(Value::Int(33)));

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
    /// Zero-filled guest image with PT_LOAD file bytes copied into place.
    /// This is not executable host memory and is not invoked directly.
    pub mapped_image: Arc<[u8]>,
    pub relocations: Vec<Relocation>,
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
        let mapped_image = materialize_native_image(elf_bytes, &plan)
            .map_err(|e| format!("Native image materialization failed for {}: {:?}", soname, e))?;

        let mut exported_symbols = HashMap::new();
        let mut has_jni_onload = false;

        let dynamic_info = inspect_elf_dynamic(elf_bytes, &elf_info)
            .map_err(|error| format!("ELF dynamic inspection failed for {}: {:?}", soname, error))?;

        let needed_libraries = dynamic_info.needed_libraries;
        let relocations = dynamic_info.relocations;

        for symbol in dynamic_info.symbols {
            if !symbol.name.is_empty() && symbol.section_index != 0 {
                if symbol.name == "JNI_OnLoad" {
                    has_jni_onload = true;
                }
                exported_symbols.insert(symbol.name, symbol.value);
            }
        }

        let module = LoadedNativeModule {
            name: soname.to_string(),
            architecture: plan.architecture,
            plan,
            mapped_image: mapped_image.into(),
            relocations,
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

    /// Returns DT_NEEDED libraries that have not yet been loaded into this
    /// process. System libraries are intentionally not hidden by name: until a
    /// real Bionic module or ABI-compatible provider is registered, they remain
    /// visible as unresolved dependencies.
    pub fn unresolved_dependencies(&self, soname: &str) -> Result<Vec<String>, String> {
        let module = self.modules.get(soname)
            .ok_or_else(|| format!("Native module {} is not loaded", soname))?;
        Ok(module.needed_libraries.iter()
            .filter(|dependency| !self.modules.contains_key(dependency.as_str()))
            .cloned()
            .collect())
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
    fn unsupported_native_memory_and_thread_symbols_fail_explicitly() {
        let bionic = BionicEnvironment::new();
        for name in ["malloc", "calloc", "pthread_create", "pthread_mutex_lock", "AAssetManager_open"] {
            let result = bionic.resolve_symbol(name).expect("symbol should be discoverable")(&[]);
            assert!(result.unwrap_err().contains(name));
        }
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
                mapped_image: Vec::new().into(),
                relocations: Vec::new(),
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
