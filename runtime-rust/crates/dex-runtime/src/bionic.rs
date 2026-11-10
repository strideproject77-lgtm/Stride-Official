//! Android Bionic libc and native library dynamic linker emulation.
//!
//! Provides the symbol resolution table, Bionic runtime environment,
//! and native library relocation resolution for ELF ARM/ARM64 shared libraries.

use std::collections::HashMap;
use std::sync::Arc;
use stride_elf_inspector::{inspect_elf, inspect_elf_dynamic, DynamicSymbol, ElfArchitecture};
use stride_native_loader::{materialize_native_image, plan_native_image, NativeImagePlan, IOS_PAGE_SIZE};
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
        self.register("strlen", unsupported_symbol("strlen"));
        self.register("strcmp", unsupported_symbol("strcmp"));
        self.register("strncmp", unsupported_symbol("strncmp"));
        self.register("strcpy", unsupported_symbol("strcpy"));
        self.register("strncpy", unsupported_symbol("strncpy"));
        self.register("strcat", unsupported_symbol("strcat"));
        self.register("strchr", unsupported_symbol("strchr"));
        self.register("strstr", unsupported_symbol("strstr"));

        // --- libc stdio ---
        self.register("printf", unsupported_symbol("printf"));
        self.register("fprintf", unsupported_symbol("fprintf"));
        self.register("sprintf", unsupported_symbol("sprintf"));
        self.register("snprintf", unsupported_symbol("snprintf"));
        self.register("sscanf", unsupported_symbol("sscanf"));
        self.register("fopen", unsupported_symbol("fopen"));
        self.register("fclose", unsupported_symbol("fclose"));
        self.register("fread", unsupported_symbol("fread"));
        self.register("fwrite", unsupported_symbol("fwrite"));
        self.register("fseek", unsupported_symbol("fseek"));
        self.register("ftell", unsupported_symbol("ftell"));
        self.register("fflush", unsupported_symbol("fflush"));

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
        self.register("clock_gettime", unsupported_symbol("clock_gettime"));
        self.register("gettimeofday", unsupported_symbol("gettimeofday"));
        self.register("nanosleep", unsupported_symbol("nanosleep"));
        self.register("usleep", unsupported_symbol("usleep"));
        self.register("time", unsupported_symbol("time"));

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
        self.register("pthread_rwlock_init", unsupported_symbol("pthread_rwlock_init"));
        self.register("pthread_rwlock_rdlock", unsupported_symbol("pthread_rwlock_rdlock"));
        self.register("pthread_rwlock_wrlock", unsupported_symbol("pthread_rwlock_wrlock"));
        self.register("pthread_rwlock_unlock", unsupported_symbol("pthread_rwlock_unlock"));
        self.register("pthread_rwlock_destroy", unsupported_symbol("pthread_rwlock_destroy"));

        // --- libdl ---
        self.register("dlopen", unsupported_symbol("dlopen"));
        self.register("dlsym", unsupported_symbol("dlsym"));
        self.register("dlclose", unsupported_symbol("dlclose"));
        self.register("dlerror", unsupported_symbol("dlerror"));

        // --- libc misc ---
        self.register("exit", |_args| Err("Native library called exit()".into()));
        self.register("raise", unsupported_symbol("raise"));
        self.register("signal", unsupported_symbol("signal"));
        self.register("getpid", unsupported_symbol("getpid"));
        self.register("getenv", unsupported_symbol("getenv"));
        self.register("setenv", unsupported_symbol("setenv"));
        self.register("rand", unsupported_symbol("rand"));
        self.register("srand", unsupported_symbol("srand"));
        self.register("atoi", unsupported_symbol("atoi"));
        self.register("atof", unsupported_symbol("atof"));
        self.register("strtol", unsupported_symbol("strtol"));
        self.register("strtod", unsupported_symbol("strtod"));

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
#[derive(Debug, Clone)]
pub struct PlannedNativeRelocation {
    pub relocation: Relocation,
    /// Offset of r_offset inside the materialized image. None is reserved for
    /// architecture-specific NONE relocations, which do not write memory.
    pub image_offset: Option<u64>,
}

/// A loaded Android shared library module.
pub struct LoadedNativeModule {
    pub name: String,
    pub architecture: ElfArchitecture,
    pub plan: NativeImagePlan,
    /// Zero-filled guest image with PT_LOAD file bytes copied into place.
    /// This is not executable host memory and is not invoked directly.
    pub mapped_image: Vec<u8>,
    pub relocations: Vec<PlannedNativeRelocation>,
    pub dynamic_symbols: Vec<DynamicSymbol>,
    pub load_bias: u64,
    pub exported_symbols: HashMap<String, u64>,
    pub needed_libraries: Vec<String>,
    pub has_jni_onload: bool,
    pub jni_onload_address: Option<u64>,
}

/// Native shared library linker coordinating ELF loading and symbol resolution.
pub struct NativeLinker {
    pub bionic: BionicEnvironment,
    pub modules: HashMap<String, LoadedNativeModule>,
    pub loaded_order: Vec<String>,
    next_guest_base: u64,
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
            next_guest_base: 0x0000_7000_0000_0000,
        }
    }

    /// Inspects and links an ELF shared library binary into the native environment.
    pub fn load_library(&mut self, soname: &str, elf_bytes: &[u8]) -> Result<&LoadedNativeModule, String> {
        let elf_info = inspect_elf(elf_bytes)
            .map_err(|e| format!("ELF inspection failed for {}: {:?}", soname, e))?;

        let unbased_plan = plan_native_image(&elf_info, 0)
            .map_err(|e| format!("Native image planning failed for {}: {:?}", soname, e))?;
        let guest_base = self.next_guest_base;
        let load_bias = guest_base.checked_sub(unbased_plan.image_virtual_start)
            .ok_or_else(|| format!("Cannot allocate guest address range for {}", soname))?;
        let plan = plan_native_image(&elf_info, load_bias)
            .map_err(|e| format!("Native image planning failed for {}: {:?}", soname, e))?;
        let mapped_image = materialize_native_image(elf_bytes, &plan)
            .map_err(|e| format!("Native image materialization failed for {}: {:?}", soname, e))?;
        let next_base = guest_base.checked_add(plan.image_size)
            .and_then(|v| v.checked_add(0x1_0000))
            .and_then(|v| v.checked_add(0xffff).map(|sum| sum & !0xffff))
            .ok_or_else(|| format!("Guest address space exhausted while loading {}", soname))?;
        self.next_guest_base = next_base;

        let mut exported_symbols = HashMap::new();
        let mut has_jni_onload = false;
        let mut jni_onload_address = None;

        let dynamic_info = inspect_elf_dynamic(elf_bytes, &elf_info)
            .map_err(|error| format!("ELF dynamic inspection failed for {}: {:?}", soname, error))?;

        let needed_libraries = dynamic_info.needed_libraries;
        let dynamic_symbols = dynamic_info.symbols.clone();
        let mut relocations = Vec::with_capacity(dynamic_info.relocations.len());
        for relocation in dynamic_info.relocations {
            let image_offset = if relocation.relocation_type == 0 {
                None
            } else {
                let belongs_to_load_segment = elf_info.segments.iter().any(|segment| {
                    segment.virtual_address.checked_add(segment.memory_size)
                        .map(|end| relocation.offset >= segment.virtual_address && relocation.offset < end)
                        .unwrap_or(false)
                });
                if !belongs_to_load_segment {
                    return Err(format!(
                        "Relocation target {:#x} in {} is outside every PT_LOAD segment",
                        relocation.offset, soname
                    ));
                }
                let offset = relocation.offset.checked_sub(plan.image_virtual_start)
                    .filter(|value| *value < plan.image_size)
                    .ok_or_else(|| format!(
                        "Relocation target {:#x} in {} is outside its mapped image",
                        relocation.offset, soname
                    ))?;
                let width = match (plan.architecture, relocation.relocation_type) {
                    (ElfArchitecture::Aarch64, 1027) => Some(8u64),
                    (ElfArchitecture::Aarch64, 257 | 1025 | 1026) => Some(8),
                    (ElfArchitecture::Arm, 23) => Some(4),
                    (ElfArchitecture::Arm, 2 | 21 | 22) => Some(4),
                    _ => None,
                };
                if let Some(width) = width {
                    let in_segment = plan.segment_copies.iter().any(|segment| {
                        offset >= segment.image_offset
                            && offset.checked_add(width).map(|end| end <= segment.image_offset + segment.memory_size).unwrap_or(false)
                    });
                    if !in_segment {
                        return Err(format!("Relocation write at {:#x} in {} crosses a PT_LOAD boundary", relocation.offset, soname));
                    }
                    let page_offset = (offset / IOS_PAGE_SIZE) * IOS_PAGE_SIZE;
                    let writable = plan.pages.iter().any(|page| page.image_offset == page_offset && page.protection.writable);
                    if !writable {
                        return Err(format!("Relocation target {:#x} in {} is not on a writable ELF page", relocation.offset, soname));
                    }
                }
                Some(offset)
            };
            relocations.push(PlannedNativeRelocation { relocation, image_offset });
        }

        for symbol in dynamic_info.symbols {
            if !symbol.name.is_empty() && symbol.section_index != 0 {
                if symbol.name == "JNI_OnLoad" {
                    has_jni_onload = true;
                    jni_onload_address = load_bias.checked_add(symbol.value);
                }
                if let Some(address) = load_bias.checked_add(symbol.value) { exported_symbols.insert(symbol.name, address); }
            }
        }

        let module = LoadedNativeModule {
            name: soname.to_string(),
            architecture: plan.architecture,
            plan,
            mapped_image,
            relocations,
            dynamic_symbols,
            load_bias,
            exported_symbols,
            needed_libraries,
            has_jni_onload,
            jni_onload_address,
        };

        self.modules.insert(soname.to_string(), module);
        if !self.loaded_order.contains(&soname.to_string()) {
            self.loaded_order.push(soname.to_string());
        }

        Ok(self.modules.get(soname).unwrap())
    }

    /// Applies supported ARM/AArch64 dynamic relocations to guest image buffers.
    /// Unsupported kinds and unresolved external symbols are returned as diagnostics.
    pub fn apply_relocations(&mut self) -> Vec<String> {
        let mut issues = Vec::new();
        let order = self.loaded_order.clone();
        for soname in order {
            let Some(snapshot) = self.modules.get(&soname) else { continue };
            let architecture = snapshot.architecture;
            let base = snapshot.load_bias;
            let relocations = snapshot.relocations.clone();
            let symbols = snapshot.dynamic_symbols.clone();
            for planned in relocations {
                let Some(offset) = planned.image_offset else { continue };
                let kind = planned.relocation.relocation_type;
                let (width, relative) = match architecture {
                    ElfArchitecture::Aarch64 => match kind {
                        1027 => (8usize, true),
                        257 | 1025 | 1026 => (8, false),
                        _ => { issues.push(format!("{}: unsupported AArch64 relocation {}", soname, kind)); continue; }
                    },
                    ElfArchitecture::Arm => match kind {
                        23 => (4usize, true),
                        2 | 21 | 22 => (4, false),
                        _ => { issues.push(format!("{}: unsupported ARM relocation {}", soname, kind)); continue; }
                    },
                    other => { issues.push(format!("{}: unsupported ELF architecture {:?}", soname, other)); continue; }
                };
                let Ok(start) = usize::try_from(offset) else {
                    issues.push(format!("{}: relocation offset conversion overflow", soname)); continue;
                };
                if self.modules.get(&soname).and_then(|m| start.checked_add(width).map(|end| end <= m.mapped_image.len())) != Some(true) {
                    issues.push(format!("{}: relocation target {:#x} is outside image", soname, offset)); continue;
                }
                let addend = if let Some(explicit) = planned.relocation.addend { explicit } else {
                    let image = &self.modules.get(&soname).unwrap().mapped_image;
                    if width == 8 {
                        i64::from_le_bytes(image[start..start+8].try_into().unwrap())
                    } else {
                        i32::from_le_bytes(image[start..start+4].try_into().unwrap()) as i64
                    }
                };
                let symbol_address = if relative { Some(0u64) } else {
                    let Some(symbol) = symbols.get(planned.relocation.symbol_index as usize) else {
                        issues.push(format!("{}: missing relocation symbol index {}", soname, planned.relocation.symbol_index)); continue;
                    };
                    if symbol.section_index != 0 {
                        base.checked_add(symbol.value)
                    } else if symbol.binding == 2 {
                        Some(0)
                    } else {
                        let mut found = None;
                        for candidate_name in &self.loaded_order {
                            if let Some(candidate) = self.modules.get(candidate_name) {
                                if let Some(address) = candidate.exported_symbols.get(&symbol.name) {
                                    found = Some(*address);
                                    if found.is_some() { break; }
                                }
                            }
                        }
                        found
                    }
                };
                let Some(symbol_address) = symbol_address else {
                    issues.push(format!("{}: unresolved relocation symbol {}", soname,
                        symbols.get(planned.relocation.symbol_index as usize).map(|s| s.name.as_str()).unwrap_or("<invalid>")));
                    continue;
                };
                let value = if relative { base as i128 + addend as i128 } else { symbol_address as i128 + addend as i128 };
                if value < 0 || (width == 4 && value > u32::MAX as i128) || value > u64::MAX as i128 {
                    issues.push(format!("{}: relocation result overflow for type {}", soname, kind)); continue;
                }
                let Some(module) = self.modules.get_mut(&soname) else { continue };
                if width == 8 { module.mapped_image[start..start+8].copy_from_slice(&(value as u64).to_le_bytes()); }
                else { module.mapped_image[start..start+4].copy_from_slice(&(value as u32).to_le_bytes()); }
            }
        }
        issues
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
                mapped_image: Vec::new(),
                relocations: Vec::new(),
                dynamic_symbols: Vec::new(),
                load_bias: 0,
                exported_symbols: syms,
                needed_libraries: Vec::new(),
                has_jni_onload: true,
                jni_onload_address: Some(0x1100),
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
