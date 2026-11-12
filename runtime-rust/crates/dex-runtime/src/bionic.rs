//! Android Bionic libc and native library dynamic linker emulation.
//!
//! Provides the symbol resolution table, Bionic runtime environment,
//! and native library relocation resolution for ELF ARM/ARM64 shared libraries.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use stride_elf_inspector::{inspect_elf, inspect_elf_dynamic, DynamicSymbol, ElfArchitecture, ElfClass, ElfByteOrder};
use stride_native_loader::{materialize_native_image, plan_native_image, NativeImagePlan, IOS_PAGE_SIZE};
use stride_elf_inspector::{Relocation, RelocationKind};
use crate::types::Value;
use crate::jni::JniEnv;

pub type BionicFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;


/// Bounded guest address space used by Bionic pointer-taking functions.
/// Addresses are opaque guest integers; host pointers are never exposed.
#[derive(Debug)]
struct GuestAddressSpace {
    allocations: BTreeMap<u64, Vec<u8>>,
    next_address: u64,
    allocated_bytes: usize,
}

impl Default for GuestAddressSpace {
    fn default() -> Self {
        Self {
            allocations: BTreeMap::new(),
            next_address: 0x0000_6000_0000_0000,
            allocated_bytes: 0,
        }
    }
}

impl GuestAddressSpace {
    const MAX_ALLOCATION: usize = 64 * 1024 * 1024;
    const MAX_TOTAL: usize = 256 * 1024 * 1024;
    const MAX_C_STRING: usize = 1024 * 1024;

    fn allocate(&mut self, requested: usize) -> Result<u64, String> {
        let size = requested.max(1);
        if size > Self::MAX_ALLOCATION
            || self.allocated_bytes.checked_add(size).filter(|n| *n <= Self::MAX_TOTAL).is_none()
        {
            return Err("guest allocation exceeds Bionic memory limit".into());
        }
        let aligned = self.next_address.checked_add(15).map(|v| v & !15)
            .ok_or("guest address space exhausted")?;
        let next = aligned.checked_add(size as u64).and_then(|v| v.checked_add(16))
            .ok_or("guest address space exhausted")?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|_| "guest allocation failed")?;
        bytes.resize(size, 0);
        self.allocations.insert(aligned, bytes);
        self.allocated_bytes += size;
        self.next_address = next;
        Ok(aligned)
    }

    fn allocation_range(&self, address: u64, length: usize) -> Result<(u64, usize), String> {
        let (&base, bytes) = self.allocations.range(..=address).next_back()
            .ok_or_else(|| format!("invalid guest pointer {address:#x}"))?;
        let offset = usize::try_from(address - base).map_err(|_| "guest pointer offset overflow")?;
        let end = offset.checked_add(length).ok_or("guest memory range overflow")?;
        if end > bytes.len() {
            return Err(format!("guest memory range {address:#x}+{length} is out of bounds"));
        }
        Ok((base, offset))
    }

    fn read(&self, address: u64, length: usize) -> Result<&[u8], String> {
        let (base, offset) = self.allocation_range(address, length)?;
        Ok(&self.allocations.get(&base).ok_or("guest allocation disappeared")?[offset..offset + length])
    }

    fn write(&mut self, address: u64, data: &[u8]) -> Result<(), String> {
        let (base, offset) = self.allocation_range(address, data.len())?;
        let bytes = self.allocations.get_mut(&base).ok_or("guest allocation disappeared")?;
        bytes[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn free(&mut self, address: u64) -> Result<(), String> {
        if address == 0 { return Ok(()); }
        let bytes = self.allocations.remove(&address)
            .ok_or_else(|| format!("free requires an allocation base, got {address:#x}"))?;
        self.allocated_bytes = self.allocated_bytes.saturating_sub(bytes.len());
        Ok(())
    }

    fn realloc(&mut self, address: u64, requested: usize) -> Result<u64, String> {
        if address == 0 { return self.allocate(requested); }
        if requested == 0 { self.free(address)?; return Ok(0); }
        let old = self.allocations.get(&address)
            .ok_or_else(|| format!("realloc requires an allocation base, got {address:#x}"))?
            .clone();
        let new_size = requested.max(1);
        if new_size > Self::MAX_ALLOCATION {
            return Err("guest reallocation exceeds per-allocation limit".into());
        }
        self.free(address)?;
        match self.allocate(new_size) {
            Ok(new_address) => {
                let copy_len = old.len().min(new_size);
                self.write(new_address, &old[..copy_len])?;
                Ok(new_address)
            }
            Err(error) => {
                self.allocations.insert(address, old.clone());
                self.allocated_bytes = self.allocated_bytes.saturating_add(old.len());
                Err(error)
            }
        }
    }

    fn c_string(&self, address: u64) -> Result<Vec<u8>, String> {
        if address == 0 { return Err("null guest C string".into()); }
        let (&base, bytes) = self.allocations.range(..=address).next_back()
            .ok_or_else(|| format!("invalid guest C string pointer {address:#x}"))?;
        let offset = usize::try_from(address - base).map_err(|_| "guest string offset overflow")?;
        let available = bytes.len().saturating_sub(offset).min(Self::MAX_C_STRING);
        let slice = &bytes[offset..offset + available];
        let end = slice.iter().position(|byte| *byte == 0)
            .ok_or("guest C string is not terminated within the readable allocation")?;
        Ok(slice[..end].to_vec())
    }
}

fn guest_pointer(value: Option<&Value>) -> Result<u64, String> {
    let pointer = value.map(Value::as_long).unwrap_or(0);
    u64::try_from(pointer).map_err(|_| "negative guest pointer".into())
}

fn guest_length(value: Option<&Value>) -> Result<usize, String> {
    let length = value.map(Value::as_long).unwrap_or(0);
    usize::try_from(length).map_err(|_| "negative or oversized guest length".into())
}

/// Builds an explicit failure for symbols whose guest ABI implementation is not
/// available. Returning a plausible pointer/status here would corrupt guest state.
fn unsupported_symbol(name: &'static str) -> impl Fn(&[Value]) -> Result<Value, String> + Send + Sync + 'static {
    move |_args| Err(format!(
        "Bionic symbol {} is not implemented; refusing placeholder success",
        name
    ))
}

/// Bionic libc/libm/liblog/libdl symbol emulation environment.
#[derive(Clone)]
pub struct BionicEnvironment {
    symbols: HashMap<String, BionicFn>,
    memory: Arc<Mutex<GuestAddressSpace>>,
}

impl Default for BionicEnvironment {
    fn default() -> Self {
        Self { symbols: HashMap::new(), memory: Arc::new(Mutex::new(GuestAddressSpace::default())) }
    }
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
        // These functions operate only on Stride-owned guest allocations. A
        // bad pointer/range is an error; host process memory is never dereferenced.
        let memory = self.memory.clone();
        self.register("malloc", move |args| {
            let size = guest_length(args.first())?;
            let address = memory.lock().map_err(|_| "guest memory lock poisoned")?.allocate(size)?;
            Ok(Value::Long(address as i64))
        });
        let memory = self.memory.clone();
        self.register("calloc", move |args| {
            let count = guest_length(args.first())?;
            let size = guest_length(args.get(1))?;
            let total = count.checked_mul(size).ok_or("calloc size overflow")?;
            let address = memory.lock().map_err(|_| "guest memory lock poisoned")?.allocate(total)?;
            Ok(Value::Long(address as i64))
        });
        let memory = self.memory.clone();
        self.register("realloc", move |args| {
            let address = guest_pointer(args.first())?;
            let size = guest_length(args.get(1))?;
            let updated = memory.lock().map_err(|_| "guest memory lock poisoned")?.realloc(address, size)?;
            Ok(Value::Long(updated as i64))
        });
        let memory = self.memory.clone();
        self.register("free", move |args| {
            let address = guest_pointer(args.first())?;
            memory.lock().map_err(|_| "guest memory lock poisoned")?.free(address)?;
            Ok(Value::Int(0))
        });
        let memory = self.memory.clone();
        self.register("memset", move |args| {
            let address = guest_pointer(args.first())?;
            let byte = args.get(1).map(Value::as_int).unwrap_or(0) as u8;
            let length = guest_length(args.get(2))?;
            if length > GuestAddressSpace::MAX_ALLOCATION {
                return Err("memset length exceeds guest memory limit".into());
            }
            let mut data = Vec::new();
            data.try_reserve_exact(length).map_err(|_| "memset allocation failed")?;
            data.resize(length, byte);
            memory.lock().map_err(|_| "guest memory lock poisoned")?.write(address, &data)?;
            Ok(Value::Long(address as i64))
        });
        for name in ["memcpy", "memmove"] {
            let memory = self.memory.clone();
            self.register(name, move |args| {
                let destination = guest_pointer(args.first())?;
                let source = guest_pointer(args.get(1))?;
                let length = guest_length(args.get(2))?;
                let mut space = memory.lock().map_err(|_| "guest memory lock poisoned")?;
                let copied = space.read(source, length)?.to_vec();
                space.write(destination, &copied)?;
                Ok(Value::Long(destination as i64))
            });
        }
        let memory = self.memory.clone();
        self.register("memcmp", move |args| {
            let left = guest_pointer(args.first())?;
            let right = guest_pointer(args.get(1))?;
            let length = guest_length(args.get(2))?;
            let space = memory.lock().map_err(|_| "guest memory lock poisoned")?;
            let a = space.read(left, length)?;
            let b = space.read(right, length)?;
            let result = a.iter().zip(b).find_map(|(x, y)| (x != y).then_some(*x as i32 - *y as i32)).unwrap_or(0);
            Ok(Value::Int(result))
        });

        // --- libc strings ---
        let memory = self.memory.clone();
        self.register("strlen", move |args| {
            let address = guest_pointer(args.first())?;
            let bytes = memory.lock().map_err(|_| "guest memory lock poisoned")?.c_string(address)?;
            Ok(Value::Long(bytes.len() as i64))
        });
        for name in ["strcmp", "strncmp"] {
            let memory = self.memory.clone();
            self.register(name, move |args| {
                let left = guest_pointer(args.first())?;
                let right = guest_pointer(args.get(1))?;
                let limit = if name == "strncmp" { guest_length(args.get(2))? } else { GuestAddressSpace::MAX_C_STRING };
                let space = memory.lock().map_err(|_| "guest memory lock poisoned")?;
                let a = space.c_string(left)?;
                let b = space.c_string(right)?;
                let result = a.iter().copied().chain(std::iter::once(0))
                    .zip(b.iter().copied().chain(std::iter::once(0)))
                    .take(limit)
                    .find_map(|(x, y)| (x != y).then_some(x as i32 - y as i32))
                    .unwrap_or(0);
                Ok(Value::Int(result))
            });
        }
        for name in ["strcpy", "strncpy"] {
            let memory = self.memory.clone();
            self.register(name, move |args| {
                let destination = guest_pointer(args.first())?;
                let source = guest_pointer(args.get(1))?;
                let mut space = memory.lock().map_err(|_| "guest memory lock poisoned")?;
                let source_bytes = space.c_string(source)?;
                let limit = if name == "strncpy" { guest_length(args.get(2))? } else { source_bytes.len() + 1 };
                if limit > GuestAddressSpace::MAX_ALLOCATION {
                    return Err("string copy length exceeds guest memory limit".into());
                }
                let mut copied = source_bytes;
                if name == "strncpy" {
                    copied.truncate(limit);
                    copied.resize(limit, 0);
                } else {
                    copied.push(0);
                }
                space.write(destination, &copied)?;
                Ok(Value::Long(destination as i64))
            });
        }
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
pub struct PlannedNativeRelocation {
    pub relocation: Relocation,
    /// Offset of r_offset inside the materialized image. None is reserved for
    /// architecture-specific NONE relocations, which do not write memory.
    pub image_offset: Option<u64>,
}

/// A loaded Android shared library module.
#[derive(Debug)]
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
    /// Preserved DT_* values needed for native constructor execution.
    pub dynamic_tags: Vec<(i64, u64)>,
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
        if self.modules.contains_key(soname) {
            return Err(format!("Native library {} is already loaded in this process", soname));
        }
        let elf_info = inspect_elf(elf_bytes)
            .map_err(|e| format!("ELF inspection failed for {}: {:?}", soname, e))?;
        if elf_info.identity.class != ElfClass::Elf64
            || elf_info.identity.architecture != ElfArchitecture::Aarch64
            || elf_info.byte_order != ElfByteOrder::Little {
            return Err(format!(
                "{} is not a little-endian ELF64 AArch64 library (found {:?}/{:?}/{:?})",
                soname, elf_info.identity.class, elf_info.identity.architecture, elf_info.byte_order
            ));
        }

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

        // Keep address allocation transactional: malformed dynamic metadata or
        // relocation records must not consume guest address space when the module
        // is rejected before registration.
        let mut exported_symbols = HashMap::new();
        let mut has_jni_onload = false;
        let mut jni_onload_address = None;

        let dynamic_info = inspect_elf_dynamic(elf_bytes, &elf_info)
            .map_err(|error| format!("ELF dynamic inspection failed for {}: {:?}", soname, error))?;

        let dynamic_tags = dynamic_info.dynamic_tags.clone();
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
                    (ElfArchitecture::Aarch64, 1027 | 257 | 1025 | 1026 | 260) => Some(8u64),
                    (ElfArchitecture::Aarch64, 258 | 261) => Some(4),
                    (ElfArchitecture::Aarch64, 259 | 262) => Some(2),
                    (ElfArchitecture::Arm, 23) => Some(4),
                    (ElfArchitecture::Arm, 2 | 21 | 22) => Some(4),
                    _ => None,
                };
                if let Some(width) = width {
                    let write_end = offset.checked_add(width)
                        .ok_or_else(|| format!("Relocation write overflow in {}", soname))?;
                    let in_segment = plan.segment_copies.iter().any(|segment| {
                        segment.image_offset.checked_add(segment.memory_size)
                            .map(|segment_end| offset >= segment.image_offset && write_end <= segment_end)
                            .unwrap_or(false)
                    });
                    if !in_segment {
                        return Err(format!("Relocation write at {:#x} in {} crosses a PT_LOAD boundary", relocation.offset, soname));
                    }
                    let first_page = (offset / IOS_PAGE_SIZE) * IOS_PAGE_SIZE;
                    let last_page = ((write_end - 1) / IOS_PAGE_SIZE) * IOS_PAGE_SIZE;
                    let mut page_offset = first_page;
                    loop {
                        let writable = plan.pages.iter().any(|page| page.image_offset == page_offset && page.protection.writable);
                        if !writable {
                            return Err(format!("Relocation target {:#x} in {} touches a non-writable ELF page", relocation.offset, soname));
                        }
                        if page_offset == last_page { break; }
                        page_offset = page_offset.checked_add(IOS_PAGE_SIZE)
                            .ok_or_else(|| format!("Relocation page overflow in {}", soname))?;
                    }
                }
                Some(offset)
            };
            relocations.push(PlannedNativeRelocation { relocation, image_offset });
        }

        for symbol in dynamic_info.symbols {
            if !symbol.name.is_empty() && symbol.section_index != 0 && symbol.binding != 0 {
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
            dynamic_tags,
            has_jni_onload,
            jni_onload_address,
        };

        self.modules.insert(soname.to_string(), module);
        self.next_guest_base = next_base;
        if !self.loaded_order.contains(&soname.to_string()) {
            self.loaded_order.push(soname.to_string());
        }

        Ok(self.modules.get(soname).unwrap())
    }

    /// Resolves an undefined symbol through the requester's DT_NEEDED closure first,
    /// preserving dependency order, then through the process-wide loaded-module scope.
    fn resolve_dependency_symbol(&self, requester: &str, symbol_name: &str) -> Option<u64> {
        let root = self.modules.get(requester)?;
        let mut pending: Vec<String> = root.needed_libraries.iter().rev().cloned().collect();
        let mut visited = HashSet::new();

        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(module) = self.modules.get(&name) {
                if let Some(address) = module.exported_symbols.get(symbol_name) {
                    return Some(*address);
                }
                for dependency in module.needed_libraries.iter().rev() {
                    pending.push(dependency.clone());
                }
            }
        }

        // Android's global linker scope can expose already-loaded global modules
        // outside the direct dependency closure. Keep this deterministic.
        for name in &self.loaded_order {
            if name == requester || visited.contains(name) {
                continue;
            }
            if let Some(address) = self.modules.get(name)
                .and_then(|module| module.exported_symbols.get(symbol_name)) {
                return Some(*address);
            }
        }
        None
    }

    /// Applies supported ARM/AArch64 relocations transactionally per module.
    /// If any relocation in a module cannot be resolved, none of that module's
    /// staged relocation writes are committed.
    pub fn apply_relocations(&mut self) -> Vec<String> {
        let mut issues = Vec::new();
        let order = self.loaded_order.clone();

        for soname in order {
            let Some(snapshot) = self.modules.get(&soname) else { continue };
            let architecture = snapshot.architecture;
            let base = snapshot.load_bias;
            let relocations = snapshot.relocations.clone();
            let symbols = snapshot.dynamic_symbols.clone();
            let mut patches: Vec<(usize, Vec<u8>)> = Vec::with_capacity(relocations.len());
            let mut module_issues = Vec::new();

            for planned in relocations {
                let Some(offset) = planned.image_offset else { continue };
                let kind = planned.relocation.relocation_type;
                let (width, relative, pc_relative) = match architecture {
                    ElfArchitecture::Aarch64 => match kind {
                        1027 => (8usize, true, false), // R_AARCH64_RELATIVE
                        257 | 1025 | 1026 => (8, false, false), // ABS64, GLOB_DAT, JUMP_SLOT
                        258 => (4, false, false), // ABS32
                        259 => (2, false, false), // ABS16
                        260 => (8, false, true), // PREL64
                        261 => (4, false, true), // PREL32
                        262 => (2, false, true), // PREL16
                        _ => {
                            module_issues.push(format!("{}: unsupported AArch64 relocation {}", soname, kind));
                            break;
                        }
                    },
                    ElfArchitecture::Arm => match kind {
                        23 => (4usize, true, false), // R_ARM_RELATIVE
                        2 | 21 | 22 => (4, false, false), // ABS32, GLOB_DAT, JUMP_SLOT
                        _ => {
                            module_issues.push(format!("{}: unsupported ARM relocation {}", soname, kind));
                            break;
                        }
                    },
                    other => {
                        module_issues.push(format!("{}: unsupported ELF architecture {:?}", soname, other));
                        break;
                    }
                };

                let Ok(start) = usize::try_from(offset) else {
                    module_issues.push(format!("{}: relocation offset conversion overflow", soname));
                    break;
                };
                let Some(end) = start.checked_add(width) else {
                    module_issues.push(format!("{}: relocation target range overflow", soname));
                    break;
                };
                let Some(image_len) = self.modules.get(&soname).map(|module| module.mapped_image.len()) else {
                    module_issues.push(format!("{}: module disappeared during relocation", soname));
                    break;
                };
                if end > image_len {
                    module_issues.push(format!("{}: relocation target {:#x} is outside image", soname, offset));
                    break;
                }

                let addend = if let Some(explicit) = planned.relocation.addend {
                    explicit
                } else {
                    let image = &self.modules.get(&soname).unwrap().mapped_image;
                    match width {
                        8 => i64::from_le_bytes(image[start..end].try_into().unwrap()),
                        4 => i32::from_le_bytes(image[start..end].try_into().unwrap()) as i64,
                        2 => i16::from_le_bytes(image[start..end].try_into().unwrap()) as i64,
                        _ => unreachable!(),
                    }
                };

                let symbol_address = if relative {
                    Some(base)
                } else {
                    let Some(symbol) = symbols.get(planned.relocation.symbol_index as usize) else {
                        module_issues.push(format!(
                            "{}: missing relocation symbol index {}",
                            soname, planned.relocation.symbol_index
                        ));
                        break;
                    };
                    if symbol.section_index != 0 {
                        base.checked_add(symbol.value)
                    } else if symbol.binding == 2 {
                        // ELF weak undefined symbols resolve to address zero.
                        Some(0)
                    } else {
                        self.resolve_dependency_symbol(&soname, &symbol.name)
                    }
                };

                let Some(symbol_address) = symbol_address else {
                    let symbol_name = symbols.get(planned.relocation.symbol_index as usize)
                        .map(|symbol| symbol.name.as_str()).unwrap_or("<invalid>");
                    module_issues.push(format!(
                        "{}: unresolved relocation symbol {}",
                        soname, symbol_name
                    ));
                    break;
                };

                let value = if relative {
                    base as i128 + addend as i128
                } else if pc_relative {
                    symbol_address as i128 + addend as i128
                        - (base as i128 + planned.relocation.offset as i128)
                } else {
                    symbol_address as i128 + addend as i128
                };
                let in_range = if pc_relative {
                    match width {
                        8 => value >= i64::MIN as i128 && value <= i64::MAX as i128,
                        4 => value >= i32::MIN as i128 && value <= i32::MAX as i128,
                        2 => value >= i16::MIN as i128 && value <= i16::MAX as i128,
                        _ => false,
                    }
                } else {
                    match width {
                        8 => value >= 0 && value <= u64::MAX as i128,
                        4 => value >= 0 && value <= u32::MAX as i128,
                        2 => value >= 0 && value <= u16::MAX as i128,
                        _ => false,
                    }
                };
                if !in_range {
                    module_issues.push(format!(
                        "{}: relocation result overflow for type {}",
                        soname, kind
                    ));
                    break;
                }

                let patch = match width {
                    8 => (value as u64).to_le_bytes().to_vec(),
                    4 => (value as u32).to_le_bytes().to_vec(),
                    2 => (value as u16).to_le_bytes().to_vec(),
                    _ => unreachable!(),
                };
                patches.push((start, patch));
            }

            if !module_issues.is_empty() {
                // Preserve the original image on any unresolved/unsupported entry.
                issues.extend(module_issues);
                continue;
            }

            if let Some(module) = self.modules.get_mut(&soname) {
                for (start, patch) in patches {
                    let end = start + patch.len();
                    module.mapped_image[start..end].copy_from_slice(&patch);
                }
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

    /// Finds a JNI native method using the short symbol first and the
    /// signature-qualified symbol required for overloaded native methods second.
    pub fn find_jni_binding(&self, class_descriptor: &str, method_name: &str) -> Option<ResolvedSymbol<'_>> {
        let class = jni_mangle_component(class_descriptor.trim_start_matches('L').trim_end_matches(';'));
        let method = jni_mangle_component(method_name);
        self.resolve_symbol(&format!("Java_{}_{}", class, method))
    }

    pub fn find_jni_binding_with_signature(
        &self, class_descriptor: &str, method_name: &str, prototype: &str
    ) -> Option<ResolvedSymbol<'_>> {
        if let Some(symbol) = self.find_jni_binding(class_descriptor, method_name) { return Some(symbol); }
        let close = prototype.find(')')?;
        let parameter_start = if prototype.starts_with('(') { 1 } else { 0 };
        if parameter_start > close { return None; }
        let parameters = &prototype[parameter_start..close];
        let class = jni_mangle_component(class_descriptor.trim_start_matches('L').trim_end_matches(';'));
        let method = jni_mangle_component(method_name);
        let signature = jni_mangle_component(parameters);
        self.resolve_symbol(&format!("Java_{}_{}__{}", class, method, signature))
    }
}

fn jni_mangle_component(input: &str) -> String {
    let mut output = String::new();
    for ch in input.chars() {
        match ch {
            '/' | '.' => output.push('_'),
            '_' => output.push_str("_1"),
            ';' => output.push_str("_2"),
            '[' => output.push_str("_3"),
            c if c.is_ascii_alphanumeric() => output.push(c),
            c => output.push_str(&format!("_0{:04x}", c as u32)),
        }
    }
    output
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
    fn guest_malloc_memset_strlen_memcpy_memcmp_and_free_work_on_guest_memory() {
        let bionic = BionicEnvironment::new();
        let malloc = bionic.resolve_symbol("malloc").unwrap();
        let first = malloc(&[Value::Long(16)]).unwrap().as_long() as u64;
        let second = malloc(&[Value::Long(16)]).unwrap().as_long() as u64;
        assert_ne!(first, second);

        let memset = bionic.resolve_symbol("memset").unwrap();
        memset(&[Value::Long(first as i64), Value::Int(b'c' as i32), Value::Long(3)]).unwrap();
        memset(&[Value::Long((first + 3) as i64), Value::Int(0), Value::Long(1)]).unwrap();
        assert_eq!(
            bionic.resolve_symbol("strlen").unwrap()(&[Value::Long(first as i64)]).unwrap(),
            Value::Long(3)
        );

        bionic.resolve_symbol("memcpy").unwrap()(&[
            Value::Long(second as i64), Value::Long(first as i64), Value::Long(4)
        ]).unwrap();
        assert_eq!(
            bionic.resolve_symbol("memcmp").unwrap()(&[
                Value::Long(first as i64), Value::Long(second as i64), Value::Long(4)
            ]).unwrap(),
            Value::Int(0)
        );

        let free = bionic.resolve_symbol("free").unwrap();
        assert_eq!(free(&[Value::Long(first as i64)]).unwrap(), Value::Int(0));
        assert!(free(&[Value::Long(first as i64)]).is_err());
        free(&[Value::Long(second as i64)]).unwrap();
    }

    #[test]
    fn unsupported_native_thread_and_asset_symbols_fail_explicitly() {
        let bionic = BionicEnvironment::new();
        for name in ["pthread_create", "pthread_mutex_lock", "AAssetManager_open"] {
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
    fn applies_aarch64_relative_relocation_to_guest_image() {
        let mut linker = NativeLinker::new();
        let load_bias = 0x0000_7000_0000_0000u64;
        linker.modules.insert(
            "libfixture.so".to_string(),
            LoadedNativeModule {
                name: "libfixture.so".to_string(),
                architecture: ElfArchitecture::Aarch64,
                plan: NativeImagePlan {
                    load_bias,
                    image_virtual_start: 0x1000,
                    image_size: 16,
                    pages: Vec::new(),
                    segment_copies: Vec::new(),
                    architecture: ElfArchitecture::Aarch64,
                },
                mapped_image: vec![0; 16],
                relocations: vec![PlannedNativeRelocation {
                    relocation: Relocation {
                        kind: RelocationKind::Rela,
                        offset: 0x1000,
                        symbol_index: 0,
                        relocation_type: 1027,
                        addend: Some(0x20),
                    },
                    image_offset: Some(0),
                }],
                dynamic_symbols: Vec::new(),
                load_bias,
                exported_symbols: HashMap::new(),
                needed_libraries: Vec::new(),
                has_jni_onload: false,
                jni_onload_address: None,
            },
        );
        linker.loaded_order.push("libfixture.so".to_string());
        assert!(linker.apply_relocations().is_empty());
        let image = &linker.modules["libfixture.so"].mapped_image;
        assert_eq!(u64::from_le_bytes(image[0..8].try_into().unwrap()), load_bias + 0x20);
    }

    #[test]
    fn jni_mangles_overloaded_parameter_descriptors() {
        assert_eq!(
            jni_mangle_component("I[Ljava/lang/String;"),
            "I_3Ljava_lang_String_2"
        );
    }

    #[test]
    fn rejects_truncated_native_library_without_registering_a_module() {
        let mut linker = NativeLinker::new();
        let error = linker.load_library("libtruncated.so", &[0u8; 8]).unwrap_err();
        assert!(error.contains("ELF inspection failed"), "{error}");
        assert!(linker.modules.is_empty());
    }

    #[test]
    fn failed_dynamic_metadata_does_not_consume_guest_address_space() {
        let mut elf = vec![0u8; 0x200];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2; // ELF64
        elf[5] = 1; // little endian
        elf[6] = 1; // ELF version
        elf[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN
        elf[18..20].copy_from_slice(&183u16.to_le_bytes()); // AArch64
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&2u16.to_le_bytes());

        // One file-backed PT_LOAD segment containing the dynamic table.
        let load = 64usize;
        elf[load..load + 4].copy_from_slice(&1u32.to_le_bytes());
        elf[load + 4..load + 8].copy_from_slice(&5u32.to_le_bytes());
        elf[load + 8..load + 16].copy_from_slice(&0u64.to_le_bytes());
        elf[load + 16..load + 24].copy_from_slice(&0x1000u64.to_le_bytes());
        elf[load + 32..load + 40].copy_from_slice(&0x200u64.to_le_bytes());
        elf[load + 40..load + 48].copy_from_slice(&0x200u64.to_le_bytes());
        elf[load + 48..load + 56].copy_from_slice(&0x1000u64.to_le_bytes());

        // PT_DYNAMIC contains only DT_NULL, so dynamic metadata validation fails
        // after the image plan has been prepared but before module registration.
        let dynamic = load + 56;
        elf[dynamic..dynamic + 4].copy_from_slice(&2u32.to_le_bytes());
        elf[dynamic + 4..dynamic + 8].copy_from_slice(&4u32.to_le_bytes());
        elf[dynamic + 8..dynamic + 16].copy_from_slice(&0x100u64.to_le_bytes());
        elf[dynamic + 16..dynamic + 24].copy_from_slice(&0x1100u64.to_le_bytes());
        elf[dynamic + 32..dynamic + 40].copy_from_slice(&16u64.to_le_bytes());
        elf[dynamic + 40..dynamic + 48].copy_from_slice(&16u64.to_le_bytes());
        elf[dynamic + 48..dynamic + 56].copy_from_slice(&8u64.to_le_bytes());

        let mut linker = NativeLinker::new();
        let initial_base = linker.next_guest_base;
        let error = linker.load_library("libbad-dynamic.so", &elf).unwrap_err();
        assert!(error.contains("ELF dynamic inspection failed"), "{error}");
        assert!(linker.modules.is_empty());
        assert_eq!(linker.next_guest_base, initial_base);
    }

}
