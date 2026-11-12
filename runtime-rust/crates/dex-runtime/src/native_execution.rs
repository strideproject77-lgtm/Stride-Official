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
use crate::arm64_guest::{
    run_aarch64, Arm64Cpu, Arm64Error, Arm64RunReport, GuestMemory, MemoryProtection,
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
    pub stack_base: u64,
    pub stack_top: u64,
}

impl NativeGuestImage {
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
        let mut image = materialize_native_image(bytes, &plan).map_err(NativeExecutionError::Materialize)?;

        let mut exports = HashMap::new();
        let (needed_libraries, relocated_symbols) = if info.dynamic_segment.is_some() {
            let dynamic = inspect_elf_dynamic(bytes, &info)
                .map_err(|error| NativeExecutionError::DynamicInfo(format!("{error:?}")))?;
            let needed = dynamic.needed_libraries.clone();
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
            (needed, count)
        } else {
            (Vec::new(), 0)
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

        let entry_is_executable = plan.pages.iter().any(|page| {
            if !page.protection.executable { return false; }
            let start = image_base.saturating_add(page.image_offset);
            let end = start.saturating_add(stride_native_loader::IOS_PAGE_SIZE);
            entry_point >= start && entry_point < end
        });
        if !entry_is_executable {
            return Err(NativeExecutionError::MissingExecutableEntry);
        }

        memory.map_protection(stack_base, GUEST_NATIVE_STACK_SIZE, MemoryProtection { readable: true, writable: true, executable: false })
            .map_err(NativeExecutionError::GuestMemory)?;

        let report = NativeImageReport {
            soname: None,
            needed_libraries,
            image_base,
            image_size: plan.image_size,
            entry_point,
            mapped_pages: plan.pages.len(),
            relocated_symbols,
        };
        Ok(Self { memory, plan, entry_point, report, exports, stack_base, stack_top })
    }

    /// Execute the ELF entry point using the current bounded interpreter subset.
    /// The caller must provide a guest stack pointer only after a writable stack
    /// region has been mapped into the same guest address space.
    pub fn run_entry(
        &mut self,
        cpu: &mut Arm64Cpu,
        stack_pointer: u64,
        instruction_budget: u64,
    ) -> Result<Arm64RunReport, NativeExecutionError> {
        if stack_pointer & 0xF != 0 || stack_pointer < self.stack_base || stack_pointer > self.stack_top {
            return Err(NativeExecutionError::InvalidEntryPoint);
        }
        cpu.pc = self.entry_point;
        cpu.sp = stack_pointer;
        cpu.x[30] = 0;
        run_aarch64(cpu, &mut self.memory, self.entry_point, instruction_budget)
            .map_err(NativeExecutionError::GuestMemory)
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
        if !self.is_executable_address(target) { return Err(NativeExecutionError::MissingExecutableEntry); }
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
}
