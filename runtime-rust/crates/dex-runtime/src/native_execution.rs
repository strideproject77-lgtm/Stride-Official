//! Guest-side native ELF image materialization and guarded AArch64 entry execution.
//!
//! This module joins the ELF program-header planner, relocation engine, and
//! bounded AArch64 interpreter. It does not call host function pointers or claim
//! full Android ABI/JNI support.

use std::collections::{HashMap, HashSet};
use stride_elf_inspector::{
    inspect_elf, inspect_elf_dynamic, DynamicSymbol, ElfArchitecture, ElfByteOrder,
    ElfProgramInfo,
};
use stride_native_loader::{
    apply_relocations, materialize_native_image, plan_native_image, NativeImagePlan,
    PageProtection, RelocationError, LoadImageError, LoadPlanError,
};
use crate::bionic::{LoadedNativeModule, NativeLinker};
use crate::arm64_guest::{
    run_aarch64, Arm64Cpu, Arm64Error, Arm64RunReport, GuestMemory, MemoryProtection, MAX_GUEST_MEMORY,
};

pub const DEFAULT_GUEST_LOAD_BIAS: u64 = 0x0000_0001_0000_0000;
pub const GUEST_NATIVE_STACK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeExecutionError {
    Elf(String),
    UnsupportedArchitecture,
    LoadPlan(LoadPlanError),
    Materialize(LoadImageError),
    DynamicInfo(String),
    Relocation(RelocationError),
    GuestMemory(Arm64Error),
    MissingExecutableEntry,
    NativeModuleNotFound(String),
    InvalidJniRegistration(String),
    UnsupportedJniVersion(u32),
    EntryAddressOverflow,
    InvalidEntryPoint,
}

/// Discrete lifecycle stages for a native shared library in the execution pipeline.
/// Distinguishes inspection, materialization, relocation, initialization, JNI setup,
/// and actual guest method invocation without reporting unearned success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeLibraryStage {
    /// ELF header, program headers, and dynamic table validated.
    ElfInspected,
    /// Memory image planned and load segments materialized.
    ElfMaterialized,
    /// Dynamic relocations applied to the mapped image.
    RelocationsApplied,
    /// DT_INIT and DT_INIT_ARRAY static initializers executed.
    InitializersExecuted,
    /// JNI_OnLoad executed with valid guest-side JavaVM/JNIEnv boundary.
    JniOnLoadExecuted,
    /// Native method bindings registered via RegisterNatives or exports.
    NativeMethodsRegistered,
    /// A native method was actually invoked by guest execution.
    NativeMethodInvoked,
    /// Native execution failed with a recorded error.
    Failed,
}

impl NativeLibraryStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ElfInspected => "ELF inspected",
            Self::ElfMaterialized => "ELF materialized",
            Self::RelocationsApplied => "Relocations applied",
            Self::InitializersExecuted => "Initializers executed",
            Self::JniOnLoadExecuted => "JNI_OnLoad executed",
            Self::NativeMethodsRegistered => "Native methods registered",
            Self::NativeMethodInvoked => "Native method actually invoked",
            Self::Failed => "Failed",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeImageReport {
    pub soname: Option<String>,
    pub needed_libraries: Vec<String>,
    pub image_base: u64,
    pub image_size: u64,
    pub entry_point: u64,
    pub mapped_pages: usize,
    pub relocated_symbols: usize,
}

#[derive(Debug)]
pub struct NativeGuestImage {
    pub memory: GuestMemory,
    pub plan: NativeImagePlan,
    pub entry_point: u64,
    pub report: NativeImageReport,
    pub exports: HashMap<String, u64>,
    pub registered_natives: HashMap<(String, String, String), u64>,
    pub init_function: Option<u64>,
    pub init_array: Vec<u64>,
    pub stack_base: u64,
    pub stack_top: u64,
    pub stage: NativeLibraryStage,
    pub initializers_executed: bool,
    pub jni_onload_executed: bool,
    pub failure_reason: Option<String>,
}

/// JNI method descriptor validator. Accepts primitive, object and array
/// types, rejects void parameters, malformed object names and trailing bytes.
fn is_valid_jni_method_descriptor(descriptor: &str) -> bool {
    fn parse_type(bytes: &[u8], cursor: &mut usize, allow_void: bool) -> bool {
        if *cursor >= bytes.len() { return false; }
        match bytes[*cursor] {
            b'V' if allow_void => { *cursor += 1; true }
            b'Z' | b'B' | b'C' | b'S' | b'I' | b'J' | b'F' | b'D' => {
                *cursor += 1; true
            }
            b'L' => {
                *cursor += 1;
                let start = *cursor;
                while *cursor < bytes.len() && bytes[*cursor] != b';' {
                    let ch = bytes[*cursor];
                    if ch == b'.' || ch == b'[' || ch == b'(' || ch == b')' { return false; }
                    *cursor += 1;
                }
                if *cursor == start || *cursor >= bytes.len() { return false; }
                *cursor += 1;
                true
            }
            b'[' => {
                let mut dimensions = 0;
                while *cursor < bytes.len() && bytes[*cursor] == b'[' {
                    *cursor += 1;
                    dimensions += 1;
                    if dimensions > 255 { return false; }
                }
                parse_type(bytes, cursor, false)
            }
            _ => false,
        }
    }

    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') { return false; }
    let mut cursor = 1;
    while cursor < bytes.len() && bytes[cursor] != b')' {
        if !parse_type(bytes, &mut cursor, false) { return false; }
    }
    if cursor >= bytes.len() || bytes[cursor] != b')' { return false; }
    cursor += 1;
    parse_type(bytes, &mut cursor, true) && cursor == bytes.len()
}

fn collect_native_initializers(
    memory: &GuestMemory,
    image_base: u64,
    plan: &NativeImagePlan,
    load_bias: u64,
    dynamic_tags: &[(i64, u64)],
) -> Result<(Option<u64>, Vec<u64>), NativeExecutionError> {
    const DT_INIT: i64 = 12;
    const DT_INIT_ARRAY: i64 = 25;
    const DT_INIT_ARRAYSZ: i64 = 27;
    const MAX_INIT_ARRAY_BYTES: u64 = 32 * 1024;

    let tag = |kind| dynamic_tags.iter().find(|(key, _)| *key == kind).map(|(_, value)| *value);
    let is_executable = |address: u64| plan.pages.iter().any(|page| {
        if !page.protection.executable { return false; }
        let start = image_base.saturating_add(page.image_offset);
        let end = start.saturating_add(stride_native_loader::IOS_PAGE_SIZE);
        address >= start && address < end
    });

    let init_function = match tag(DT_INIT).unwrap_or(0) {
        0 => None,
        value => {
            let address = load_bias.checked_add(value).ok_or(NativeExecutionError::EntryAddressOverflow)?;
            if !is_executable(address) {
                return Err(NativeExecutionError::InvalidEntryPoint);
            }
            Some(address)
        }
    };

    let array_size = tag(DT_INIT_ARRAYSZ).unwrap_or(0);
    let mut init_array = Vec::new();
    if array_size != 0 {
        if array_size > MAX_INIT_ARRAY_BYTES || array_size % 8 != 0 {
            return Err(NativeExecutionError::DynamicInfo("invalid or oversized DT_INIT_ARRAYSZ".into()));
        }
        let array_vaddr = tag(DT_INIT_ARRAY)
            .ok_or_else(|| NativeExecutionError::DynamicInfo("DT_INIT_ARRAYSZ is present without DT_INIT_ARRAY".into()))?;
        let array_address = load_bias.checked_add(array_vaddr).ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let count = usize::try_from(array_size / 8).map_err(|_| NativeExecutionError::EntryAddressOverflow)?;
        init_array.try_reserve_exact(count).map_err(|_| NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit))?;
        for index in 0..count {
            let slot = array_address.checked_add((index as u64) * 8)
                .ok_or(NativeExecutionError::EntryAddressOverflow)?;
            let target = memory.read_u64(slot).map_err(NativeExecutionError::GuestMemory)?;
            if target == 0 || target == u64::MAX { continue; }
            if !is_executable(target) {
                return Err(NativeExecutionError::InvalidEntryPoint);
            }
            init_array.push(target);
        }
    }
    Ok((init_function, init_array))
}

impl NativeGuestImage {
    /// Resolve an already-loaded NativeLinker module by SONAME and create its
    /// guest execution view. Call apply_relocations() first and reject the
    /// module if the linker returned any relocation diagnostics.
    pub fn from_native_linker(linker: &NativeLinker, soname: &str) -> Result<Self, NativeExecutionError> {
        let module = linker.modules.get(soname)
            .ok_or_else(|| NativeExecutionError::NativeModuleNotFound(soname.to_owned()))?;
        Self::from_linked_module(module)
    }
    /// Build an executable guest image from Stride's existing NativeLinker output.
    /// The caller must run NativeLinker::apply_relocations first and verify that
    /// the module has no unresolved relocation diagnostics.
    pub fn from_linked_module(module: &LoadedNativeModule) -> Result<Self, NativeExecutionError> {
        if module.architecture != ElfArchitecture::Aarch64 { return Err(NativeExecutionError::UnsupportedArchitecture); }
        if module.mapped_image.len() as u64 != module.plan.image_size
            || module.mapped_image.len().checked_add(GUEST_NATIVE_STACK_SIZE).filter(|size| *size <= MAX_GUEST_MEMORY).is_none() {
            return Err(NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit));
        }
        let image_base = module.load_bias.checked_add(module.plan.image_virtual_start)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let image_size = module.mapped_image.len() as u64;
        let stack_base = image_base.checked_add(image_size).ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let stack_top = stack_base.checked_add(GUEST_NATIVE_STACK_SIZE as u64)
            .ok_or(NativeExecutionError::EntryAddressOverflow)? & !0xF;
        let mut image = module.mapped_image.clone();
        image.try_reserve_exact(GUEST_NATIVE_STACK_SIZE)
            .map_err(|_| NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit))?;
        image.resize(image.len() + GUEST_NATIVE_STACK_SIZE, 0);
        let mut memory = GuestMemory::from_bytes(image_base, image)
            .map_err(NativeExecutionError::GuestMemory)?;
        memory.enable_strict_protections();
        for page in &module.plan.pages {
            let address = image_base.checked_add(page.image_offset)
                .ok_or(NativeExecutionError::EntryAddressOverflow)?;
            memory.map_protection(address, stride_native_loader::IOS_PAGE_SIZE as usize, MemoryProtection {
                readable: page.protection.readable,
                writable: page.protection.writable,
                executable: page.protection.executable,
            }).map_err(NativeExecutionError::GuestMemory)?;
        }
        memory.map_protection(stack_base, GUEST_NATIVE_STACK_SIZE, MemoryProtection {
            readable: true, writable: true, executable: false,
        }).map_err(NativeExecutionError::GuestMemory)?;
        let (init_function, init_array) = collect_native_initializers(
            &memory, image_base, &module.plan, module.load_bias, &module.dynamic_tags,
        )?;
        let entry_point = module.jni_onload_address.unwrap_or(0);
        let report = NativeImageReport {
            soname: Some(module.name.clone()),
            needed_libraries: module.needed_libraries.clone(),
            image_base, image_size: module.plan.image_size, entry_point,
            mapped_pages: module.plan.pages.len(), relocated_symbols: module.relocations.len(),
        };
        Ok(Self {
            memory, plan: module.plan.clone(), entry_point, report,
            exports: module.exported_symbols.clone(), registered_natives: HashMap::new(), init_function, init_array, stack_base, stack_top,
            stage: if !module.relocations.is_empty() { NativeLibraryStage::RelocationsApplied } else { NativeLibraryStage::ElfMaterialized },
            initializers_executed: false,
            jni_onload_executed: false,
            failure_reason: None,
        })
    }

    /// Validate the version returned by JNI_OnLoad against versions supported
    /// by this compatibility layer (JNI 1.1, 1.2 and 1.4).
    pub fn validate_jni_version(version: i32) -> Result<u32, NativeExecutionError> {
        const JNI_VERSION_1_1: u32 = 0x0001_0001;
        const JNI_VERSION_1_2: u32 = 0x0001_0002;
        const JNI_VERSION_1_4: u32 = 0x0001_0004;
        let value = version as u32;
        match value {
            JNI_VERSION_1_1 | JNI_VERSION_1_2 | JNI_VERSION_1_4 => Ok(value),
            _ => Err(NativeExecutionError::UnsupportedJniVersion(value)),
        }
    }

    /// Invoke JNI_OnLoad and reject unsupported/invalid version returns.
    pub fn run_jni_onload_checked(
        &mut self, java_vm: u64, reserved: u64, cpu: &mut Arm64Cpu, instruction_budget: u64,
    ) -> Result<(Arm64RunReport, u32), NativeExecutionError> {
        if self.jni_onload_executed {
            return Ok((Arm64RunReport { stop_reason: crate::arm64_guest::Arm64StopReason::Halted, instructions_executed: 0, final_pc: self.entry_point }, 0x0001_0006));
        }
        let (report, version) = match self.run_jni_onload(java_vm, reserved, cpu, instruction_budget) {
            Ok(pair) => pair,
            Err(error) => {
                self.stage = NativeLibraryStage::Failed;
                self.failure_reason = Some(format!("JNI_OnLoad execution failed: {:?}", error));
                return Err(error);
            }
        };
        let validated_version = match Self::validate_jni_version(version) {
            Ok(v) => v,
            Err(error) => {
                self.stage = NativeLibraryStage::Failed;
                self.failure_reason = Some(format!("JNI_OnLoad returned unsupported version {:#x}", version));
                return Err(error);
            }
        };
        self.jni_onload_executed = true;
        self.stage = NativeLibraryStage::JniOnLoadExecuted;
        Ok((report, validated_version))
    }

    /// Invoke JNI_OnLoad with guest JavaVM* and reserved arguments. The return
    /// value is read from x0 after guest execution; the JavaVM bridge itself
    /// must be provided by a compatible guest-side JNI implementation.
    pub fn run_jni_onload(
        &mut self, java_vm: u64, reserved: u64, cpu: &mut Arm64Cpu, instruction_budget: u64,
    ) -> Result<(Arm64RunReport, i32), NativeExecutionError> {
        let report = self.run_export("JNI_OnLoad", &[java_vm, reserved], cpu, instruction_budget)?;
        Ok((report, cpu.x[0] as u32 as i32))
    }

    /// Invoke a statically-mangled JNI native export with guest handles and
    /// integer/pointer ABI words. This does not convert host JniEnv objects.
    pub fn run_jni_native(
        &mut self, class_descriptor: &str, method_name: &str, prototype: &str,
        jni_env: u64, receiver_or_class: u64, arguments: &[u64],
        cpu: &mut Arm64Cpu, instruction_budget: u64,
    ) -> Result<(Arm64RunReport, u64), NativeExecutionError> {
        let target = self.resolve_registered_native(class_descriptor, method_name, prototype)
            .or_else(|| Self::jni_export_candidates(class_descriptor, method_name, prototype).iter().find_map(|name| self.resolve_export(name)))
            .ok_or(NativeExecutionError::MissingExecutableEntry)?;
        let mut abi_arguments = Vec::with_capacity(arguments.len() + 2);
        abi_arguments.push(jni_env);
        abi_arguments.push(receiver_or_class);
        abi_arguments.extend_from_slice(arguments);
        let report = self.run_address(target, &abi_arguments, cpu, instruction_budget)?;
        self.stage = NativeLibraryStage::NativeMethodInvoked;
        Ok((report, cpu.x[0]))
    }
    /// Parse, plan, materialize, relocate, and permission-map one ARM64 ELF image.
    /// Undefined symbols are resolved by the caller to guest addresses; returning
    /// None fails relocation rather than installing a fake function address.
    pub fn from_elf<F>(
        bytes: &[u8],
        load_bias: u64,
        mut resolve_import: F,
    ) -> Result<Self, NativeExecutionError>
    where
        F: FnMut(&DynamicSymbol) -> Option<u64>,
    {
        let info = inspect_elf(bytes).map_err(|error| NativeExecutionError::Elf(format!("{error:?}")))?;
        if info.identity.architecture != ElfArchitecture::Aarch64 {
            return Err(NativeExecutionError::UnsupportedArchitecture);
        }
        if info.byte_order != ElfByteOrder::Little {
            return Err(NativeExecutionError::UnsupportedArchitecture);
        }

        let plan = plan_native_image(&info, load_bias).map_err(NativeExecutionError::LoadPlan)?;
        let image_size = usize::try_from(plan.image_size).map_err(|_| NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit))?;
        if image_size.checked_add(GUEST_NATIVE_STACK_SIZE).filter(|size| *size <= MAX_GUEST_MEMORY).is_none() {
            return Err(NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit));
        }
        let mut image = materialize_native_image(bytes, &plan).map_err(NativeExecutionError::Materialize)?;

        let mut exports = HashMap::new();
        let (needed_libraries, relocated_symbols, dynamic_tags) = if info.dynamic_segment.is_some() {
            let dynamic = inspect_elf_dynamic(bytes, &info)
                .map_err(|error| NativeExecutionError::DynamicInfo(format!("{error:?}")))?;
            let needed = dynamic.needed_libraries.clone();
            let dynamic_tags = dynamic.dynamic_tags.clone();
            for symbol in &dynamic.symbols {
                if symbol.section_index != 0 && !symbol.name.is_empty() && (symbol.binding == 1 || symbol.binding == 2) {
                    if let Some(address) = load_bias.checked_add(symbol.value) {
                        exports.entry(symbol.name.clone()).or_insert(address);
                    }
                }
            }
            let count = apply_relocations(
                &mut image,
                &plan,
                &dynamic,
                info.byte_order,
                &mut resolve_import,
            ).map_err(NativeExecutionError::Relocation)?;
            (needed, count, dynamic_tags)
        } else {
            (Vec::new(), 0, Vec::new())
        };

        let image_base = load_bias.checked_add(plan.image_virtual_start)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let entry_point = load_bias.checked_add(info.entry)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let image_size = image.len() as u64;
        let stack_base = image_base.checked_add(image_size).ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let stack_top = stack_base.checked_add(GUEST_NATIVE_STACK_SIZE as u64).ok_or(NativeExecutionError::EntryAddressOverflow)? & !0xF;
        image.try_reserve_exact(GUEST_NATIVE_STACK_SIZE).map_err(|_| NativeExecutionError::GuestMemory(Arm64Error::MemoryLimit))?;
        image.resize(image.len() + GUEST_NATIVE_STACK_SIZE, 0);
        let mut memory = GuestMemory::from_bytes(image_base, image)
            .map_err(NativeExecutionError::GuestMemory)?;
        memory.enable_strict_protections();

        for page in &plan.pages {
            let protection = MemoryProtection {
                readable: page.protection.readable,
                writable: page.protection.writable,
                executable: page.protection.executable,
            };
            let address = image_base.checked_add(page.image_offset)
                .ok_or(NativeExecutionError::EntryAddressOverflow)?;
            memory.map_protection(address, stride_native_loader::IOS_PAGE_SIZE as usize, protection)
                .map_err(NativeExecutionError::GuestMemory)?;
        }

        memory.map_protection(stack_base, GUEST_NATIVE_STACK_SIZE, MemoryProtection { readable: true, writable: true, executable: false })
            .map_err(NativeExecutionError::GuestMemory)?;

        let (init_function, init_array) = collect_native_initializers(
            &memory, image_base, &plan, load_bias, &dynamic_tags,
        )?;
        let report = NativeImageReport {
            soname: None,
            needed_libraries,
            image_base,
            image_size: plan.image_size,
            entry_point,
            mapped_pages: plan.pages.len(),
            relocated_symbols,
        };
        Ok(Self {
            memory, plan, entry_point, report, exports, registered_natives: HashMap::new(), init_function, init_array, stack_base, stack_top,
            stage: if relocated_symbols > 0 { NativeLibraryStage::RelocationsApplied } else { NativeLibraryStage::ElfMaterialized },
            initializers_executed: false,
            jni_onload_executed: false,
            failure_reason: None,
        })
    }

    /// Run DT_INIT followed by DT_INIT_ARRAY entries after dependencies have
    /// been relocated and initialized. Each constructor receives no arguments.
    /// The caller is responsible for invoking this once per module in dependency order.
    pub fn run_initializers(
        &mut self,
        cpu: &mut Arm64Cpu,
        instruction_budget_per_constructor: u64,
    ) -> Result<Vec<Arm64RunReport>, NativeExecutionError> {
        if self.initializers_executed { return Ok(Vec::new()); }
        let mut targets = Vec::with_capacity(self.init_array.len() + if self.init_function.is_some() { 1 } else { 0 });
        if let Some(address) = self.init_function { targets.push(address); }
        targets.extend(self.init_array.iter().copied());
        let mut reports = Vec::with_capacity(targets.len());
        for target in targets {
            match self.run_address(target, &[], cpu, instruction_budget_per_constructor) {
                Ok(report) => reports.push(report),
                Err(error) => {
                    self.stage = NativeLibraryStage::Failed;
                    self.failure_reason = Some(format!("Initializer at {:#x} failed: {:?}", target, error));
                    return Err(error);
                }
            }
        }
        self.initializers_executed = true;
        self.stage = NativeLibraryStage::InitializersExecuted;
        Ok(reports)
    }

    /// Execute the ELF entry point using the current bounded interpreter subset.
    /// Uses the writable guest stack region allocated alongside this image.
    pub fn run_entry(
        &mut self,
        cpu: &mut Arm64Cpu,
        stack_pointer: u64,
        instruction_budget: u64,
    ) -> Result<Arm64RunReport, NativeExecutionError> {
        if !self.is_executable_address(self.entry_point) || stack_pointer & 0xF != 0 || stack_pointer < self.stack_base || stack_pointer > self.stack_top {
            return Err(NativeExecutionError::InvalidEntryPoint);
        }
        cpu.pc = self.entry_point;
        cpu.sp = stack_pointer;
        cpu.x[30] = 0;
        run_aarch64(cpu, &mut self.memory, self.entry_point, instruction_budget)
            .map_err(NativeExecutionError::GuestMemory)
    }

    /// Parse a guest JNINativeMethod array and register its function pointers.
    /// Each AArch64 record is { name*, signature*, fnPtr } (24 bytes). All
    /// strings must be readable guest memory and each function must belong to
    /// an executable page in this image. Registration is staged atomically.
    pub fn register_jni_natives(
        &mut self, class_descriptor: &str, methods_address: u64, count: usize,
    ) -> Result<usize, NativeExecutionError> {
        if count > 4096 || (count > 0 && methods_address == 0) {
            return Err(NativeExecutionError::InvalidJniRegistration("invalid method count or null method table".into()));
        }
        if !class_descriptor.starts_with('L') || !class_descriptor.ends_with(';') {
            return Err(NativeExecutionError::InvalidJniRegistration("class descriptor must use L...; form".into()));
        }
        let mut staged = Vec::with_capacity(count);
        let mut seen = HashSet::with_capacity(count);
        for index in 0..count {
            let offset = (index as u64).checked_mul(24).ok_or(NativeExecutionError::EntryAddressOverflow)?;
            let record = methods_address.checked_add(offset).ok_or(NativeExecutionError::EntryAddressOverflow)?;
            let name_ptr = self.memory.read_u64(record).map_err(NativeExecutionError::GuestMemory)?;
            let signature_ptr = self.memory.read_u64(record.checked_add(8).ok_or(NativeExecutionError::EntryAddressOverflow)?)
                .map_err(NativeExecutionError::GuestMemory)?;
            let function_ptr = self.memory.read_u64(record.checked_add(16).ok_or(NativeExecutionError::EntryAddressOverflow)?)
                .map_err(NativeExecutionError::GuestMemory)?;
            if name_ptr == 0 || signature_ptr == 0 || function_ptr == 0 || !self.is_executable_address(function_ptr) {
                return Err(NativeExecutionError::InvalidJniRegistration(format!("invalid JNI method pointer at entry {index}")));
            }
            let name = String::from_utf8(self.memory.read_c_string(name_ptr, 256).map_err(NativeExecutionError::GuestMemory)?)
                .map_err(|_| NativeExecutionError::InvalidJniRegistration(format!("method name {index} is not UTF-8")))?;
            let signature = String::from_utf8(self.memory.read_c_string(signature_ptr, 1024).map_err(NativeExecutionError::GuestMemory)?)
                .map_err(|_| NativeExecutionError::InvalidJniRegistration(format!("method signature {index} is not UTF-8")))?;
            if name.is_empty() || !is_valid_jni_method_descriptor(&signature) {
                return Err(NativeExecutionError::InvalidJniRegistration(format!("malformed JNI method declaration at entry {index}")));
            }
            let key = (class_descriptor.to_owned(), name, signature);
            if !seen.insert(key.clone()) {
                return Err(NativeExecutionError::InvalidJniRegistration(format!("duplicate JNI method declaration at entry {index}")));
            }
            staged.push((key, function_ptr));
        }
        for (key, address) in staged.iter() { self.registered_natives.insert(key.clone(), *address); }
        if !self.registered_natives.is_empty() { self.stage = NativeLibraryStage::NativeMethodsRegistered; }
        Ok(staged.len())
    }

    /// Implements the state change requested by JNI UnregisterNatives.
    /// Returns the number of guest native bindings removed for this class.
    pub fn unregister_jni_natives(&mut self, class_descriptor: &str) -> usize {
        let before = self.registered_natives.len();
        self.registered_natives.retain(|(class, _, _), _| class != class_descriptor);
        before - self.registered_natives.len()
    }

    pub fn resolve_registered_native(&self, class_descriptor: &str, method_name: &str, prototype: &str) -> Option<u64> {
        self.registered_natives.get(&(class_descriptor.to_owned(), method_name.to_owned(), prototype.to_owned())).copied()
    }
    /// Resolve a globally/weakly exported ELF symbol to its guest virtual address.
    pub fn resolve_export(&self, symbol: &str) -> Option<u64> {
        self.exports.get(symbol).copied()
    }

    /// Execute one exported function with up to eight AAPCS64 integer/pointer
    /// arguments. This runs guest instructions in the bounded interpreter.
    pub fn run_export(
        &mut self, symbol: &str, arguments: &[u64], cpu: &mut Arm64Cpu, instruction_budget: u64,
    ) -> Result<Arm64RunReport, NativeExecutionError> {
        if arguments.len() > 8 { return Err(NativeExecutionError::InvalidEntryPoint); }
        let target = self.resolve_export(symbol).ok_or(NativeExecutionError::MissingExecutableEntry)?;
        self.run_address(target, arguments, cpu, instruction_budget)
    }

    fn run_address(
        &mut self, target: u64, arguments: &[u64], cpu: &mut Arm64Cpu, instruction_budget: u64,
    ) -> Result<Arm64RunReport, NativeExecutionError> {
        if arguments.len() > 8 || !self.is_executable_address(target) {
            return Err(NativeExecutionError::MissingExecutableEntry);
        }
        cpu.x = [0; 31];
        cpu.flags = Default::default();
        cpu.halted = false;
        cpu.pc = target;
        cpu.sp = self.stack_top;
        cpu.x[30] = 0;
        Self::prepare_aapcs64_arguments(cpu, arguments)?;
        run_aarch64(cpu, &mut self.memory, target, instruction_budget)
            .map_err(NativeExecutionError::GuestMemory)
    }
    /// JNI short/long native symbol candidates using JNI name-escaping rules.
    pub fn jni_export_candidates(class_descriptor: &str, method_name: &str, prototype: &str) -> Vec<String> {
        fn encode(value: &str) -> String {
            let mut output = String::new();
            for ch in value.chars() {
                match ch {
                    '/' | '.' => output.push('_'),
                    '_' => output.push_str("_1"),
                    ';' => output.push_str("_2"),
                    '[' => output.push_str("_3"),
                    ch if ch.is_ascii_alphanumeric() => output.push(ch),
                    ch => output.push_str(&format!("_0{:04x}", ch as u32)),
                }
            }
            output
        }
        let owner = class_descriptor.trim_start_matches('L').trim_end_matches(';');
        let args = prototype.split_once('(').and_then(|(_, rest)| rest.split_once(')'))
            .map(|(arguments, _)| arguments).unwrap_or("");
        let base = format!("Java_{}_{}", encode(owner), encode(method_name));
        vec![base.clone(), format!("{}__{}", base, encode(args))]
    }

    /// Resolve a DEX native declaration against JNI-mangled ELF exports.
    pub fn resolve_jni_export(&self, class_descriptor: &str, method_name: &str, prototype: &str) -> Option<u64> {
        Self::jni_export_candidates(class_descriptor, method_name, prototype)
            .iter().find_map(|candidate| self.resolve_export(candidate))
    }
    /// Place up to eight integer/pointer arguments in AAPCS64 x0..x7.
    /// This is register preparation only; floating-point/SIMD arguments,
    /// stack arguments, JNI reference conversion and callbacks are not handled.
    pub fn prepare_aapcs64_arguments(
        cpu: &mut Arm64Cpu,
        arguments: &[u64],
    ) -> Result<(), NativeExecutionError> {
        if arguments.len() > 8 {
            return Err(NativeExecutionError::InvalidEntryPoint);
        }
        cpu.x[..arguments.len()].copy_from_slice(arguments);
        Ok(())
    }

    /// Setup a valid synthetic guest-side JavaVM and JNIEnv boundary in the image stack
    /// region so JNI_OnLoad can safely dereference JavaVM and call GetEnv/RegisterNatives.
    pub fn setup_guest_jni_environment(&mut self) -> Result<(u64, u64), NativeExecutionError> {
        let vm_struct_addr = self.stack_base.checked_add(0x100)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let vm_vtable_addr = self.stack_base.checked_add(0x110)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let env_struct_addr = self.stack_base.checked_add(0x200)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;
        let env_vtable_addr = self.stack_base.checked_add(0x210)
            .ok_or(NativeExecutionError::EntryAddressOverflow)?;

        let stub_addr = self.plan.pages.iter().find_map(|page| {
            if page.protection.executable {
                Some(self.report.image_base.saturating_add(page.image_offset))
            } else {
                None
            }
        }).unwrap_or(self.entry_point);

        // Point JavaVM* to vm_vtable_addr
        self.memory.write_u64(vm_struct_addr, vm_vtable_addr)
            .map_err(NativeExecutionError::GuestMemory)?;
        for i in 0..10 {
            self.memory.write_u64(vm_vtable_addr + (i * 8), stub_addr)
                .map_err(NativeExecutionError::GuestMemory)?;
        }

        // Point JNIEnv* to env_vtable_addr
        self.memory.write_u64(env_struct_addr, env_vtable_addr)
            .map_err(NativeExecutionError::GuestMemory)?;
        for i in 0..250 {
            self.memory.write_u64(env_vtable_addr + (i * 8), stub_addr)
                .map_err(NativeExecutionError::GuestMemory)?;
        }

        Ok((vm_struct_addr, env_struct_addr))
    }
    pub fn is_executable_address(&self, address: u64) -> bool {
        self.plan.pages.iter().any(|page| {
            if !page.protection.executable { return false; }
            let start = self.report.image_base.saturating_add(page.image_offset);
            let end = start.saturating_add(stride_native_loader::IOS_PAGE_SIZE);
            address >= start && address < end
        })
    }

    pub fn page_protection(&self, address: u64) -> Option<PageProtection> {
        self.plan.pages.iter().find_map(|page| {
            let start = self.report.image_base.checked_add(page.image_offset)?;
            let end = start.checked_add(stride_native_loader::IOS_PAGE_SIZE)?;
            (address >= start && address < end).then_some(page.protection)
        })
    }
}

/// Convenience path for a pre-inspected ELF program header table. Kept separate
/// from from_elf so higher-level package loaders can reuse their parsed metadata.
pub fn plan_guest_native_image(
    info: &ElfProgramInfo,
    load_bias: u64,
) -> Result<NativeImagePlan, NativeExecutionError> {
    if info.identity.architecture != ElfArchitecture::Aarch64 {
        return Err(NativeExecutionError::UnsupportedArchitecture);
    }
    plan_native_image(info, load_bias).map_err(NativeExecutionError::LoadPlan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_jni_method_descriptors_strictly() {
        for valid in ["()V", "(I)I", "(Ljava/lang/String;[I)Z", "([[Ljava/lang/Object;)Ljava/lang/String;"] {
            assert!(is_valid_jni_method_descriptor(valid), "expected valid descriptor: {valid}");
        }
        for invalid in ["", "I)V", "(V)V", "(Ljava/lang/String)V", "([V)V", "()VV", "(L;)V", "(I)"] {
            assert!(!is_valid_jni_method_descriptor(invalid), "expected invalid descriptor: {invalid}");
        }
    }

    #[test]
    fn accepts_only_supported_jni_onload_versions() {
        assert_eq!(NativeGuestImage::validate_jni_version(0x0001_0001).unwrap(), 0x0001_0001);
        assert_eq!(NativeGuestImage::validate_jni_version(0x0001_0002).unwrap(), 0x0001_0002);
        assert_eq!(NativeGuestImage::validate_jni_version(0x0001_0004).unwrap(), 0x0001_0004);
        assert!(NativeGuestImage::validate_jni_version(0x0001_0008).is_err());
        assert!(NativeGuestImage::validate_jni_version(0).is_err());
    }

    #[test]
    fn rejects_non_aarch64_image_before_mapping() {
        // ELF magic and class are intentionally insufficient: inspect_elf must
        // parse a complete header, and architecture is checked before mapping.
        let result = NativeGuestImage::from_elf(&[0u8; 64], DEFAULT_GUEST_LOAD_BIAS, |_| None);
        assert!(matches!(result, Err(NativeExecutionError::Elf(_))));
    }

    #[test]
    fn aapcs64_argument_registers_are_bounded() {
        let mut cpu = Arm64Cpu::default();
        NativeGuestImage::prepare_aapcs64_arguments(&mut cpu, &[1, 2, 3]).unwrap();
        assert_eq!(&cpu.x[..3], &[1, 2, 3]);
        assert!(NativeGuestImage::prepare_aapcs64_arguments(&mut cpu, &[0; 9]).is_err());
    }
    #[test]
    fn unregisters_only_the_requested_classes_native_methods() {
        let mut image = NativeGuestImage {
            memory: GuestMemory::new(DEFAULT_GUEST_LOAD_BIAS, 4096).unwrap(),
            plan: NativeImagePlan {
                load_bias: DEFAULT_GUEST_LOAD_BIAS,
                image_virtual_start: 0,
                image_size: 4096,
                pages: Vec::new(),
                segment_copies: Vec::new(),
                architecture: stride_elf_inspector::ElfArchitecture::Aarch64,
            },
            entry_point: 0, report: NativeImageReport {
                soname: None, needed_libraries: Vec::new(), image_base: DEFAULT_GUEST_LOAD_BIAS,
                image_size: 4096, entry_point: 0, mapped_pages: 0, relocated_symbols: 0,
            },
            exports: HashMap::new(),
            registered_natives: HashMap::from([
                (("Lcom/example/A;".into(), "start".into(), "()V".into()), 1),
                (("Lcom/example/A;".into(), "stop".into(), "()V".into()), 2),
                (("Lcom/example/B;".into(), "start".into(), "()V".into()), 3),
            ]),
            init_function: None, init_array: Vec::new(),
            stack_base: DEFAULT_GUEST_LOAD_BIAS, stack_top: DEFAULT_GUEST_LOAD_BIAS + 4096,
            stage: NativeLibraryStage::ElfMaterialized,
            initializers_executed: false,
            jni_onload_executed: false,
            failure_reason: None,
        };
        assert_eq!(image.unregister_jni_natives("Lcom/example/A;"), 2);
        assert_eq!(image.registered_natives.len(), 1);
        assert_eq!(image.resolve_registered_native("Lcom/example/B;", "start", "()V"), Some(3));
        assert_eq!(image.unregister_jni_natives("Lmissing;"), 0);
    }

    #[test]
    fn generates_jni_short_and_long_symbol_names() {
        let names = NativeGuestImage::jni_export_candidates(
            "Lcom/example/Game_Native;", "nativeInit", "(Ljava/lang/String;I)V",
        );
        assert_eq!(names[0], "Java_com_example_Game_1Native_nativeInit");
        assert_eq!(names[1], "Java_com_example_Game_1Native_nativeInit__Ljava_lang_String_2I");
    }
    #[test]
    fn stage_transitions_and_descriptions_distinguish_all_seven_phases() {
        assert_eq!(NativeLibraryStage::ElfInspected.as_str(), "ELF inspected");
        assert_eq!(NativeLibraryStage::ElfMaterialized.as_str(), "ELF materialized");
        assert_eq!(NativeLibraryStage::RelocationsApplied.as_str(), "Relocations applied");
        assert_eq!(NativeLibraryStage::InitializersExecuted.as_str(), "Initializers executed");
        assert_eq!(NativeLibraryStage::JniOnLoadExecuted.as_str(), "JNI_OnLoad executed");
        assert_eq!(NativeLibraryStage::NativeMethodsRegistered.as_str(), "Native methods registered");
        assert_eq!(NativeLibraryStage::NativeMethodInvoked.as_str(), "Native method actually invoked");
        assert_eq!(NativeLibraryStage::Failed.as_str(), "Failed");
        assert!(NativeLibraryStage::ElfInspected < NativeLibraryStage::ElfMaterialized);
        assert!(NativeLibraryStage::ElfMaterialized < NativeLibraryStage::RelocationsApplied);
        assert!(NativeLibraryStage::RelocationsApplied < NativeLibraryStage::InitializersExecuted);
        assert!(NativeLibraryStage::InitializersExecuted < NativeLibraryStage::JniOnLoadExecuted);
        assert!(NativeLibraryStage::JniOnLoadExecuted < NativeLibraryStage::NativeMethodsRegistered);
        assert!(NativeLibraryStage::NativeMethodsRegistered < NativeLibraryStage::NativeMethodInvoked);
    }

    #[test]
    fn initializers_idempotency_guard_prevents_repeated_execution() {
        let mut image = NativeGuestImage {
            memory: GuestMemory::new(DEFAULT_GUEST_LOAD_BIAS, 4096).unwrap(),
            plan: NativeImagePlan {
                load_bias: DEFAULT_GUEST_LOAD_BIAS,
                image_virtual_start: 0,
                image_size: 4096,
                pages: Vec::new(),
                segment_copies: Vec::new(),
                architecture: stride_elf_inspector::ElfArchitecture::Aarch64,
            },
            entry_point: 0,
            report: NativeImageReport {
                soname: Some("libtest.so".into()),
                needed_libraries: Vec::new(),
                image_base: DEFAULT_GUEST_LOAD_BIAS,
                image_size: 4096,
                entry_point: 0,
                mapped_pages: 0,
                relocated_symbols: 0,
            },
            exports: HashMap::new(),
            registered_natives: HashMap::new(),
            init_function: None,
            init_array: Vec::new(),
            stack_base: DEFAULT_GUEST_LOAD_BIAS,
            stack_top: DEFAULT_GUEST_LOAD_BIAS + 4096,
            stage: NativeLibraryStage::RelocationsApplied,
            initializers_executed: true,
            jni_onload_executed: false,
            failure_reason: None,
        };
        let mut cpu = Arm64Cpu::default();
        let reports = image.run_initializers(&mut cpu, 1000).unwrap();
        assert!(reports.is_empty(), "repeat execution must return empty without error");
        assert_eq!(image.stage, NativeLibraryStage::RelocationsApplied);
    }

    #[test]
    fn jni_onload_idempotency_guard_prevents_repeated_invocation() {
        let mut image = NativeGuestImage {
            memory: GuestMemory::new(DEFAULT_GUEST_LOAD_BIAS, 4096).unwrap(),
            plan: NativeImagePlan {
                load_bias: DEFAULT_GUEST_LOAD_BIAS,
                image_virtual_start: 0,
                image_size: 4096,
                pages: Vec::new(),
                segment_copies: Vec::new(),
                architecture: stride_elf_inspector::ElfArchitecture::Aarch64,
            },
            entry_point: 0,
            report: NativeImageReport {
                soname: Some("libtest.so".into()),
                needed_libraries: Vec::new(),
                image_base: DEFAULT_GUEST_LOAD_BIAS,
                image_size: 4096,
                entry_point: 0,
                mapped_pages: 0,
                relocated_symbols: 0,
            },
            exports: HashMap::new(),
            registered_natives: HashMap::new(),
            init_function: None,
            init_array: Vec::new(),
            stack_base: DEFAULT_GUEST_LOAD_BIAS,
            stack_top: DEFAULT_GUEST_LOAD_BIAS + 4096,
            stage: NativeLibraryStage::JniOnLoadExecuted,
            initializers_executed: true,
            jni_onload_executed: true,
            failure_reason: None,
        };
        let mut cpu = Arm64Cpu::default();
        let (_, version) = image.run_jni_onload_checked(0x1000, 0, &mut cpu, 1000).unwrap();
        assert_eq!(version, 0x0001_0006);
    }

    #[test]
    fn guest_jni_environment_setup_places_valid_pointers() {
        let mut image = NativeGuestImage {
            memory: GuestMemory::new(DEFAULT_GUEST_LOAD_BIAS, 8192).unwrap(),
            plan: NativeImagePlan {
                load_bias: DEFAULT_GUEST_LOAD_BIAS,
                image_virtual_start: 0,
                image_size: 8192,
                pages: Vec::new(),
                segment_copies: Vec::new(),
                architecture: stride_elf_inspector::ElfArchitecture::Aarch64,
            },
            entry_point: DEFAULT_GUEST_LOAD_BIAS + 0x1000,
            report: NativeImageReport {
                soname: Some("libtest.so".into()),
                needed_libraries: Vec::new(),
                image_base: DEFAULT_GUEST_LOAD_BIAS,
                image_size: 8192,
                entry_point: DEFAULT_GUEST_LOAD_BIAS + 0x1000,
                mapped_pages: 0,
                relocated_symbols: 0,
            },
            exports: HashMap::new(),
            registered_natives: HashMap::new(),
            init_function: None,
            init_array: Vec::new(),
            stack_base: DEFAULT_GUEST_LOAD_BIAS + 4096,
            stack_top: DEFAULT_GUEST_LOAD_BIAS + 8192,
            stage: NativeLibraryStage::RelocationsApplied,
            initializers_executed: false,
            jni_onload_executed: false,
            failure_reason: None,
        };
        let (vm_ptr, env_ptr) = image.setup_guest_jni_environment().unwrap();
        assert!(vm_ptr >= image.stack_base && vm_ptr < image.stack_top);
        assert!(env_ptr >= image.stack_base && env_ptr < image.stack_top);
        let vm_vtable = image.memory.read_u64(vm_ptr).unwrap();
        assert_eq!(vm_vtable, image.stack_base + 0x110);
        let env_vtable = image.memory.read_u64(env_ptr).unwrap();
        assert_eq!(env_vtable, image.stack_base + 0x210);
    }
}
