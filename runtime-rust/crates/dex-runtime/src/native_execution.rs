//! Guest-side native ELF image materialization and guarded AArch64 entry execution.
//!
//! This module joins the ELF program-header planner, relocation engine, and
//! bounded AArch64 interpreter. It does not call host function pointers or claim
//! full Android ABI/JNI support.

use std::collections::HashMap;
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
    EntryAddressOverflow,
    InvalidEntryPoint,
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
        })
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
        Ok(Self { memory, plan, entry_point, report, exports, registered_natives: HashMap::new(), init_function, init_array, stack_base, stack_top })
    }

    /// Run DT_INIT followed by DT_INIT_ARRAY entries after dependencies have
    /// been relocated and initialized. Each constructor receives no arguments.
    /// The caller is responsible for invoking this once per module in dependency order.
    pub fn run_initializers(
        &mut self,
        cpu: &mut Arm64Cpu,
        instruction_budget_per_constructor: u64,
    ) -> Result<Vec<Arm64RunReport>, NativeExecutionError> {
        let mut targets = Vec::with_capacity(self.init_array.len() + if self.init_function.is_some() { 1 } else { 0 });
        if let Some(address) = self.init_function { targets.push(address); }
        targets.extend(self.init_array.iter().copied());
        let mut reports = Vec::with_capacity(targets.len());
        for target in targets {
            reports.push(self.run_address(target, &[], cpu, instruction_budget_per_constructor)?);
        }
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
            if name.is_empty() || !signature.starts_with('(') || !signature.contains(')') {
                return Err(NativeExecutionError::InvalidJniRegistration(format!("malformed JNI method declaration at entry {index}")));
            }
            staged.push(((class_descriptor.to_owned(), name, signature), function_ptr));
        }
        for (key, address) in staged.iter() { self.registered_natives.insert(key.clone(), *address); }
        Ok(staged.len())
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
    fn generates_jni_short_and_long_symbol_names() {
        let names = NativeGuestImage::jni_export_candidates(
            "Lcom/example/Game_Native;", "nativeInit", "(Ljava/lang/String;I)V",
        );
        assert_eq!(names[0], "Java_com_example_Game_1Native_nativeInit");
        assert_eq!(names[1], "Java_com_example_Game_1Native_nativeInit__Ljava_lang_String_2I");
    }
}
