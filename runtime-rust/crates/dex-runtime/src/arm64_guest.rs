//! Bounded AArch64 guest instruction interpreter.
//!
//! This is an execution foundation, not a complete ARMv8-A emulator and not
//! native execution on the iOS host. It interprets a deliberately documented
//! integer/control-flow/memory subset over Stride-owned guest memory. Unknown
//! instructions fail closed; host pointers and host code execution are never used.

pub const MAX_GUEST_MEMORY: usize = 64 * 1024 * 1024;
pub const MAX_INSTRUCTIONS_PER_RUN: u64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryProtection { pub readable: bool, pub writable: bool, pub executable: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtectedRange { start: u64, end: u64, protection: MemoryProtection }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm64Error {
    EmptyCode,
    InvalidEntryPoint,
    MisalignedProgramCounter,
    InstructionLimit,
    UnsupportedInstruction(u32),
    MemoryOutOfBounds,
    AddressOverflow,
    InvalidRegister,
    InvalidBranchTarget,
    MemoryLimit,

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Arm64Flags {
    pub negative: bool,
    pub zero: bool,
    pub carry: bool,
    pub overflow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm64Cpu {
    /// X0..X30. X31 is interpreted as SP or the zero register per instruction.
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub flags: Arm64Flags,
    pub instructions_executed: u64,
    pub halted: bool,
    /// Per-interpreter exclusive reservation used by LDXR/LDAXR and
    /// STXR/STLXR. Cross-thread reservation arbitration is not implemented yet.
    pub exclusive_monitor: Option<(u64, usize)>,
    /// Thread Local Storage pointer (TPIDR_EL0) for Bionic libc TLS and stack canary.
    pub tpidr_el0: u64,
    /// V0..V31 SIMD/FP registers (128-bit).
    pub v: [u128; 32],
}

impl Default for Arm64Cpu {
    fn default() -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            pc: 0,
            flags: Arm64Flags::default(),
            instructions_executed: 0,
            halted: false,
            exclusive_monitor: None,
            tpidr_el0: 0,
            v: [0; 32],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestMemory {
    base: u64,
    bytes: Vec<u8>,
    protected_ranges: Vec<ProtectedRange>,
    strict_protections: bool,
}

impl GuestMemory {
    pub fn new(base: u64, size: usize) -> Result<Self, Arm64Error> {
        if size > MAX_GUEST_MEMORY { return Err(Arm64Error::MemoryLimit); }
        base.checked_add(size as u64).ok_or(Arm64Error::AddressOverflow)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|_| Arm64Error::MemoryLimit)?;
        bytes.resize(size, 0);
        Ok(Self { base, bytes, protected_ranges: Vec::new(), strict_protections: false })
    }

    pub fn from_bytes(base: u64, bytes: Vec<u8>) -> Result<Self, Arm64Error> {
        if bytes.len() > MAX_GUEST_MEMORY { return Err(Arm64Error::MemoryLimit); }
        base.checked_add(bytes.len() as u64).ok_or(Arm64Error::AddressOverflow)?;
        Ok(Self { base, bytes, protected_ranges: Vec::new(), strict_protections: false })
    }

    pub fn base(&self) -> u64 { self.base }
    pub fn len(&self) -> usize { self.bytes.len() }
    pub fn is_empty(&self) -> bool { self.bytes.is_empty() }

    /// Enable page/segment permission enforcement. Unmapped bytes become inaccessible.
    pub fn enable_strict_protections(&mut self) { self.strict_protections = true; }

    /// Assign permissions to a non-overlapping guest range.
    pub fn map_protection(&mut self, address: u64, length: usize, protection: MemoryProtection) -> Result<(), Arm64Error> {
        if length == 0 { return Err(Arm64Error::MemoryOutOfBounds); }
        let range = self.range(address, length)?;
        let end = self.base.checked_add(range.end as u64).ok_or(Arm64Error::AddressOverflow)?;
        if self.protected_ranges.iter().any(|region| address < region.end && end > region.start) {
            return Err(Arm64Error::MemoryOutOfBounds);
        }
        self.protected_ranges.push(ProtectedRange { start: address, end, protection });
        self.protected_ranges.sort_by_key(|region| region.start);
        Ok(())
    }

    fn check_permission(&self, address: u64, width: usize, executable: bool, write: bool) -> Result<(), Arm64Error> {
        if !self.strict_protections { return Ok(()); }
        let end = address.checked_add(width as u64).ok_or(Arm64Error::AddressOverflow)?;
        let region = self.protected_ranges.iter().find(|region| address >= region.start && end <= region.end)
            .ok_or(Arm64Error::MemoryOutOfBounds)?;
        let allowed = if executable { region.protection.executable } else if write { region.protection.writable } else { region.protection.readable };
        if allowed { Ok(()) } else { Err(Arm64Error::MemoryOutOfBounds) }
    }

    fn range(&self, address: u64, width: usize) -> Result<std::ops::Range<usize>, Arm64Error> {
        let offset = address.checked_sub(self.base).ok_or(Arm64Error::MemoryOutOfBounds)?;
        let start = usize::try_from(offset).map_err(|_| Arm64Error::MemoryOutOfBounds)?;
        let end = start.checked_add(width).filter(|end| *end <= self.bytes.len())
            .ok_or(Arm64Error::MemoryOutOfBounds)?;
        Ok(start..end)
    }

    pub fn read_u8(&self, address: u64) -> Result<u8, Arm64Error> {
        self.check_permission(address, 1, false, false)?;
        Ok(self.bytes[self.range(address, 1)?.start])
    }
    pub fn read_u16(&self, address: u64) -> Result<u16, Arm64Error> {
        self.check_permission(address, 2, false, false)?;
        let r = self.range(address, 2)?;
        Ok(u16::from_le_bytes(self.bytes[r].try_into().map_err(|_| Arm64Error::MemoryOutOfBounds)?))
    }
    pub fn read_u32(&self, address: u64) -> Result<u32, Arm64Error> {
        self.check_permission(address, 4, false, false)?;
        let r = self.range(address, 4)?;
        Ok(u32::from_le_bytes(self.bytes[r].try_into().map_err(|_| Arm64Error::MemoryOutOfBounds)?))
    }
    pub fn read_u64(&self, address: u64) -> Result<u64, Arm64Error> {
        self.check_permission(address, 8, false, false)?;
        let r = self.range(address, 8)?;
        Ok(u64::from_le_bytes(self.bytes[r].try_into().map_err(|_| Arm64Error::MemoryOutOfBounds)?))
    }
    pub fn write_u8(&mut self, address: u64, value: u8) -> Result<(), Arm64Error> {
        self.check_permission(address, 1, false, true)?;
        let i = self.range(address, 1)?.start;
        self.bytes[i] = value;
        Ok(())
    }
    pub fn write_u16(&mut self, address: u64, value: u16) -> Result<(), Arm64Error> {
        self.check_permission(address, 2, false, true)?;
        let r = self.range(address, 2)?;
        self.bytes[r].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    pub fn write_u32(&mut self, address: u64, value: u32) -> Result<(), Arm64Error> {
        self.check_permission(address, 4, false, true)?;
        let r = self.range(address, 4)?;
        self.bytes[r].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    /// Read a bounded NUL-terminated guest string from a readable mapping.
    pub fn read_c_string(&self, address: u64, max_len: usize) -> Result<Vec<u8>, Arm64Error> {
        if max_len == 0 || max_len > 4096 { return Err(Arm64Error::MemoryLimit); }
        let mut output = Vec::new();
        output.try_reserve_exact(max_len.min(256)).map_err(|_| Arm64Error::MemoryLimit)?;
        for offset in 0..max_len {
            let current = address.checked_add(offset as u64).ok_or(Arm64Error::AddressOverflow)?;
            let byte = self.read_u8(current)?;
            if byte == 0 { return Ok(output); }
            output.push(byte);
        }
        Err(Arm64Error::MemoryOutOfBounds)
    }
    pub fn write_u64(&mut self, address: u64, value: u64) -> Result<(), Arm64Error> {
        self.check_permission(address, 8, false, true)?;
        let r = self.range(address, 8)?;
        self.bytes[r].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn fetch_instruction(&self, address: u64) -> Result<u32, Arm64Error> {
        self.check_permission(address, 4, true, false)?;
        let r = self.range(address, 4)?;
        Ok(u32::from_le_bytes(self.bytes[r].try_into().map_err(|_| Arm64Error::MemoryOutOfBounds)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm64StopReason { Halted, InstructionBudget }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arm64RunReport {
    pub stop_reason: Arm64StopReason,
    pub instructions_executed: u64,
    pub final_pc: u64,
}

fn sign_extend(value: u64, bits: u32) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}

fn add_flags(a: u64, b: u64, result: u64, is_64: bool) -> Arm64Flags {
    let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
    let a = a & mask;
    let b = b & mask;
    let r = result & mask;
    let carry = if is_64 { (a as u128 + b as u128) > u64::MAX as u128 }
        else { a as u32 as u64 + b as u32 as u64 > u32::MAX as u64 };
    let sign = if is_64 { 1u64 << 63 } else { 1u64 << 31 };
    let overflow = ((!(a ^ b) & (a ^ r)) & sign) != 0;
    Arm64Flags { negative: r & sign != 0, zero: r == 0, carry, overflow }
}

fn sub_flags(a: u64, b: u64, result: u64, is_64: bool) -> Arm64Flags {
    let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
    let a = a & mask;
    let b = b & mask;
    let r = result & mask;
    let sign = if is_64 { 1u64 << 63 } else { 1u64 << 31 };
    let overflow = (((a ^ b) & (a ^ r)) & sign) != 0;
    Arm64Flags { negative: r & sign != 0, zero: r == 0, carry: a >= b, overflow }
}

fn decode_logical_immediate(is_64: bool, n: u32, immr: u32, imms: u32) -> Option<u64> {
    let width = if is_64 { 64u32 } else { 32u32 };
    if !is_64 && n != 0 { return None; }
    let combined = (n << 6) | ((!imms) & 0x3f);
    if combined == 0 { return None; }
    let len = 31 - combined.leading_zeros();
    if len < 1 { return None; }
    let element_size = 1u32.checked_shl(len)?;
    if element_size > width { return None; }
    let levels = element_size - 1;
    let s = imms & levels;
    let r = immr & levels;
    if s == levels { return None; }
    let ones = (1u64 << (s + 1)) - 1;
    let element_mask = if element_size == 64 { u64::MAX } else { (1u64 << element_size) - 1 };
    let element = if r == 0 {
        ones & element_mask
    } else {
        ((ones >> r) | (ones << (element_size - r))) & element_mask
    };
    let mut result = 0u64;
    let mut shift = 0u32;
    while shift < width {
        result |= element << shift;
        shift += element_size;
    }
    Some(result)
}

fn condition_holds(flags: Arm64Flags, cond: u32) -> bool {
    match cond {
        0 => flags.zero,
        1 => !flags.zero,
        2 => flags.carry,
        3 => !flags.carry,
        4 => flags.negative,
        5 => !flags.negative,
        6 => flags.overflow,
        7 => !flags.overflow,
        8 => flags.carry && !flags.zero,
        9 => !flags.carry || flags.zero,
        10 => flags.negative == flags.overflow,
        11 => flags.negative != flags.overflow,
        12 => !flags.zero && flags.negative == flags.overflow,
        13 => flags.zero || flags.negative != flags.overflow,
        14 => true,
        _ => false,
    }
}

fn read_reg(cpu: &Arm64Cpu, index: u32, is_64: bool, sp_allowed: bool) -> Result<u64, Arm64Error> {
    let value = match index {
        0..=30 => cpu.x[index as usize],
        31 if sp_allowed => cpu.sp,
        31 => 0,
        _ => return Err(Arm64Error::InvalidRegister),
    };
    Ok(if is_64 { value } else { value as u32 as u64 })
}

fn write_reg(cpu: &mut Arm64Cpu, index: u32, value: u64, is_64: bool, sp_allowed: bool) -> Result<(), Arm64Error> {
    let value = if is_64 { value } else { value as u32 as u64 };
    match index {
        0..=30 => cpu.x[index as usize] = value,
        31 if sp_allowed => cpu.sp = value,
        31 => {},
        _ => return Err(Arm64Error::InvalidRegister),
    }
    Ok(())
}

/// Execute a bounded AArch64 subset from a guest code image.
/// Supported: NOP, MOVZ/MOVK, ADD/SUB immediate, B/BL, B.cond,
/// CBZ/CBNZ, BR/BLR/RET, and unsigned-offset LDR/STR (32/64-bit).
pub fn run_aarch64(
    cpu: &mut Arm64Cpu,
    memory: &mut GuestMemory,
    code_start: u64,
    instruction_budget: u64,
) -> Result<Arm64RunReport, Arm64Error> {
    if instruction_budget == 0 { return Err(Arm64Error::InstructionLimit); }
    if instruction_budget > MAX_INSTRUCTIONS_PER_RUN { return Err(Arm64Error::InstructionLimit); }
    if memory.is_empty() { return Err(Arm64Error::EmptyCode); }
    if cpu.pc == 0 { cpu.pc = code_start; }
    if cpu.pc & 3 != 0 { return Err(Arm64Error::MisalignedProgramCounter); }
    let code_end = memory.base().checked_add(memory.len() as u64).ok_or(Arm64Error::AddressOverflow)?;
    if cpu.pc < memory.base() || cpu.pc.checked_add(4).filter(|end| *end <= code_end).is_none() {
        return Err(Arm64Error::InvalidEntryPoint);
    }
    cpu.halted = false;
    let mut executed_this_run = 0u64;

    while executed_this_run < instruction_budget {
        if cpu.pc & 3 != 0 { return Err(Arm64Error::MisalignedProgramCounter); }
        let instruction = memory.fetch_instruction(cpu.pc)?;
        let current_pc = cpu.pc;
        let next_pc = current_pc.checked_add(4).ok_or(Arm64Error::AddressOverflow)?;
        cpu.pc = next_pc;
        cpu.instructions_executed = cpu.instructions_executed.saturating_add(1);
        executed_this_run += 1;

        if instruction == 0xD503201F { continue; } // NOP

        // Unconditional immediate branch: B / BL.
        if instruction & 0x7C00_0000 == 0x1400_0000 {
            let link = instruction & 0x8000_0000 != 0;
            let delta = sign_extend((instruction & 0x03FF_FFFF) as u64, 26) << 2;
            if link { cpu.x[30] = next_pc; }
            cpu.pc = current_pc.checked_add_signed(delta).ok_or(Arm64Error::AddressOverflow)?;
            continue;
        }

        // Conditional immediate branch.
        if instruction & 0xFF00_0010 == 0x5400_0000 {
            let cond = instruction & 0xF;
            if condition_holds(cpu.flags, cond) {
                let delta = sign_extend(((instruction >> 5) & 0x7FFFF) as u64, 19) << 2;
                cpu.pc = current_pc.checked_add_signed(delta).ok_or(Arm64Error::AddressOverflow)?;
            }
            continue;
        }

        // Compare and branch: CBZ / CBNZ.
        if instruction & 0x7E00_0000 == 0x3400_0000 {
            let is_64 = instruction >> 31 != 0;
            let nonzero = instruction & 0x0100_0000 != 0;
            let rt = instruction & 0x1F;
            let value = read_reg(cpu, rt, is_64, false)?;
            if (value != 0) == nonzero {
                let delta = sign_extend(((instruction >> 5) & 0x7FFFF) as u64, 19) << 2;
                cpu.pc = current_pc.checked_add_signed(delta).ok_or(Arm64Error::AddressOverflow)?;
            }
            continue;
        }

        // MOVZ / MOVK wide-immediate instructions.
        if instruction & 0x7F80_0000 == 0x5280_0000 || instruction & 0x7F80_0000 == 0x7280_0000 {
            let is_64 = instruction >> 31 != 0;
            let keep = instruction & 0x7F80_0000 == 0x7280_0000;
            let hw = (instruction >> 21) & 0x3;
            if !is_64 && hw > 1 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let shift = hw * 16;
            let imm = ((instruction >> 5) & 0xFFFF) as u64;
            let rd = instruction & 0x1F;
            let mask = 0xFFFFu64 << shift;
            let value = if keep {
                (read_reg(cpu, rd, is_64, false)? & !mask) | (imm << shift)
            } else { imm << shift };
            write_reg(cpu, rd, value, is_64, false)?;
            continue;
        }

        // ADD/SUB immediate, including flag-setting variants.
        if instruction & 0x1F00_0000 == 0x1100_0000 {
            let is_64 = instruction >> 31 != 0;
            let subtract = instruction & 0x4000_0000 != 0;
            let set_flags = instruction & 0x2000_0000 != 0;
            let shift = if instruction & 0x0040_0000 != 0 { 12 } else { 0 };
            let imm = (((instruction >> 10) & 0xFFF) as u64) << shift;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let a = read_reg(cpu, rn, is_64, !set_flags)?;
            let result = if subtract { a.wrapping_sub(imm) } else { a.wrapping_add(imm) };
            if set_flags {
                cpu.flags = if subtract { sub_flags(a, imm, result, is_64) }
                    else { add_flags(a, imm, result, is_64) };
                write_reg(cpu, rd, result, is_64, false)?;
            } else {
                write_reg(cpu, rd, result, is_64, true)?;
            }
            continue;
        }

        // ADR / ADRP address generation.
        if instruction & 0x9F00_0000 == 0x1000_0000 || instruction & 0x9F00_0000 == 0x9000_0000 {
            let page_relative = instruction & 0x8000_0000 != 0;
            let immlo = ((instruction >> 29) & 0x3) as u64;
            let immhi = ((instruction >> 5) & 0x7FFFF) as u64;
            let immediate = sign_extend((immhi << 2) | immlo, 21);
            let base = if page_relative { current_pc & !0xFFF } else { current_pc };
            let delta = if page_relative { immediate.checked_shl(12).ok_or(Arm64Error::AddressOverflow)? } else { immediate };
            let value = base.checked_add_signed(delta).ok_or(Arm64Error::AddressOverflow)?;
            write_reg(cpu, instruction & 0x1F, value, true, false)?;
            continue;
        }

        // ADD/SUB shifted register (LSL, LSR and ASR forms).
        if instruction & 0x1F20_0000 == 0x0B00_0000 {
            let is_64 = instruction >> 31 != 0;
            let subtract = instruction & 0x4000_0000 != 0;
            let set_flags = instruction & 0x2000_0000 != 0;
            let shift_type = (instruction >> 22) & 0x3;
            let amount = (instruction >> 10) & 0x3F;
            let width = if is_64 { 64 } else { 32 };
            if amount >= width || shift_type == 3 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let rm = (instruction >> 16) & 0x1F;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let source = read_reg(cpu, rm, is_64, false)?;
            let rhs = match shift_type {
                0 => source.wrapping_shl(amount),
                1 => source.wrapping_shr(amount),
                2 => if is_64 { ((source as i64) >> amount) as u64 } else { ((source as u32 as i32) >> amount) as u32 as u64 },
                _ => unreachable!(),
            };
            let lhs = read_reg(cpu, rn, is_64, !set_flags)?;
            let result = if subtract { lhs.wrapping_sub(rhs) } else { lhs.wrapping_add(rhs) };
            if set_flags {
                cpu.flags = if subtract { sub_flags(lhs, rhs, result, is_64) } else { add_flags(lhs, rhs, result, is_64) };
                write_reg(cpu, rd, result, is_64, false)?;
            } else { write_reg(cpu, rd, result, is_64, true)?; }
            continue;
        }

        // Logical shifted register: AND, ORR, EOR and ANDS (including MOV aliases).
        if instruction & 0x1F00_0000 == 0x0A00_0000 {
            let is_64 = instruction >> 31 != 0;
            let opc = (instruction >> 29) & 0x3;
            let invert = instruction & 0x0020_0000 != 0;
            let shift_type = (instruction >> 22) & 0x3;
            let amount = (instruction >> 10) & 0x3F;
            let width = if is_64 { 64 } else { 32 };
            if amount >= width || shift_type == 3 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let rm = (instruction >> 16) & 0x1F;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let source = read_reg(cpu, rm, is_64, false)?;
            let mut rhs = match shift_type {
                0 => source.wrapping_shl(amount),
                1 => source.wrapping_shr(amount),
                2 => if is_64 { ((source as i64) >> amount) as u64 } else { ((source as u32 as i32) >> amount) as u32 as u64 },
                _ => unreachable!(),
            };
            if invert { rhs = !rhs; }
            let lhs = read_reg(cpu, rn, is_64, false)?;
            let result = match opc { 0 => lhs & rhs, 1 => lhs | rhs, 2 => lhs ^ rhs, 3 => lhs & rhs, _ => unreachable!() };
            if opc == 3 {
                let sign = if is_64 { 1u64 << 63 } else { 1u64 << 31 };
                cpu.flags = Arm64Flags { negative: result & sign != 0, zero: (if is_64 { result } else { result as u32 as u64 }) == 0, carry: false, overflow: false };
            }
            write_reg(cpu, rd, result, is_64, false)?;
            continue;
        }

        // LDR literal / LDRSW literal.
        if instruction & 0x3B00_0000 == 0x1800_0000 && instruction & 0x0400_0000 == 0 {
            let opc = instruction >> 30;
            if opc == 3 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let delta = sign_extend(((instruction >> 5) & 0x7FFFF) as u64, 19) << 2;
            let address = current_pc.checked_add_signed(delta).ok_or(Arm64Error::AddressOverflow)?;
            let value = match opc {
                0 => memory.read_u32(address)? as u64,
                1 => memory.read_u64(address)?,
                2 => (memory.read_u32(address)? as i32 as i64) as u64,
                _ => unreachable!(),
            };
            write_reg(cpu, instruction & 0x1F, value, opc != 0, false)?;
            continue;
        }

        // Load/store pair (LDP/STP), signed-offset, pre-index and post-index.
        if instruction & 0x3A00_0000 == 0x2800_0000 && instruction & 0x0400_0000 == 0 {
            let is_64 = instruction >> 31 != 0;
            let load = instruction & 0x0040_0000 != 0;
            let mode = (instruction >> 23) & 0x3;
            if mode == 0 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let width = if is_64 { 8u64 } else { 4u64 };
            let immediate = sign_extend(((instruction >> 15) & 0x7F) as u64, 7)
                .checked_mul(width as i64).ok_or(Arm64Error::AddressOverflow)?;
            let rn = (instruction >> 5) & 0x1F;
            let rt2 = (instruction >> 10) & 0x1F;
            let rt = instruction & 0x1F;
            let base = read_reg(cpu, rn, true, true)?;
            let address = if mode == 3 { base.checked_add_signed(immediate).ok_or(Arm64Error::AddressOverflow)? } else { base };
            let second = address.checked_add(width).ok_or(Arm64Error::AddressOverflow)?;
            if load {
                let first_value = if is_64 { memory.read_u64(address)? } else { memory.read_u32(address)? as u64 };
                let second_value = if is_64 { memory.read_u64(second)? } else { memory.read_u32(second)? as u64 };
                write_reg(cpu, rt, first_value, is_64, false)?;
                write_reg(cpu, rt2, second_value, is_64, false)?;
            } else {
                let first_value = read_reg(cpu, rt, is_64, false)?;
                let second_value = read_reg(cpu, rt2, is_64, false)?;
                if is_64 { memory.write_u64(address, first_value)?; memory.write_u64(second, second_value)?; }
                else { memory.write_u32(address, first_value as u32)?; memory.write_u32(second, second_value as u32)?; }
            }
            if mode == 1 || mode == 3 {
                let updated = base.checked_add_signed(immediate).ok_or(Arm64Error::AddressOverflow)?;
                write_reg(cpu, rn, updated, true, true)?;
            }
            continue;
        }
        // Bitfield operations: SBFM, BFM and UBFM (including LSL/LSR/ASR aliases).
        if instruction & 0x1F80_0000 == 0x1300_0000 {
            let is_64 = instruction >> 31 != 0;
            let width = if is_64 { 64u32 } else { 32u32 };
            let opc = (instruction >> 29) & 0x3;
            let n = (instruction >> 22) & 1;
            let immr = (instruction >> 16) & 0x3F;
            let imms = (instruction >> 10) & 0x3F;
            if (is_64 && n != 1) || (!is_64 && n != 0) || opc == 3 {
                return Err(Arm64Error::UnsupportedInstruction(instruction));
            }
            let r = immr & (width - 1);
            let s = imms & (width - 1);
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let source = read_reg(cpu, rn, is_64, false)?;
            let ones = if s == 63 { u64::MAX } else { (1u64 << (s + 1)) - 1 };
            let width_mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            let write_mask = if r == 0 {
                ones
            } else {
                ((ones >> r) | (ones << (width - r))) & width_mask
            };
            let rotated = if r == 0 {
                source & width_mask
            } else {
                ((source >> r) | (source << (width - r))) & width_mask
            };
            let mut result = rotated & write_mask;
            match opc {
                0 => {
                    if s < width - 1 && ((source >> s) & 1) != 0 {
                        result |= width_mask & !((1u64 << (s + 1)) - 1);
                    }
                }
                1 => {
                    let old = read_reg(cpu, rd, is_64, false)?;
                    result = (old & !write_mask) | result;
                }
                2 => {}
                _ => unreachable!(),
            }
            write_reg(cpu, rd, result, is_64, false)?;
            continue;
        }

        // Logical immediate: AND/ORR/EOR/ANDS and MOV/ TST aliases.
        if instruction & 0x1F80_0000 == 0x1200_0000 {
            let is_64 = instruction >> 31 != 0;
            let opc = (instruction >> 29) & 0x3;
            let n = (instruction >> 22) & 1;
            let immr = (instruction >> 16) & 0x3F;
            let imms = (instruction >> 10) & 0x3F;
            let mask = decode_logical_immediate(is_64, n, immr, imms)
                .ok_or(Arm64Error::UnsupportedInstruction(instruction))?;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let lhs = read_reg(cpu, rn, is_64, false)?;
            let result = match opc {
                0 => lhs & mask,
                1 => lhs | mask,
                2 => lhs ^ mask,
                3 => lhs & mask,
                _ => unreachable!(),
            };
            if opc == 3 {
                let sign = if is_64 { 1u64 << 63 } else { 1u64 << 31 };
                cpu.flags = Arm64Flags {
                    negative: result & sign != 0,
                    zero: if is_64 { result == 0 } else { result as u32 == 0 },
                    carry: false,
                    overflow: false,
                };
            }
            write_reg(cpu, rd, result, is_64, false)?;
            continue;
        }

        // Conditional select family: CSEL, CSINC, CSINV and CSNEG.
        if instruction & 0x1FE0_0C00 == 0x1A80_0000 {
            let is_64 = instruction >> 31 != 0;
            let op = (instruction >> 29) & 0x3;
            let op2 = (instruction >> 10) & 0x3;
            if op != 0 { return Err(Arm64Error::UnsupportedInstruction(instruction)); }
            let rm = (instruction >> 16) & 0x1F;
            let cond = (instruction >> 12) & 0xF;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;
            let selected = if condition_holds(cpu.flags, cond) {
                read_reg(cpu, rn, is_64, false)?
            } else {
                let value = read_reg(cpu, rm, is_64, false)?;
                match op2 {
                    0 => value,
                    1 => value.wrapping_add(1),
                    2 => !value,
                    3 => value.wrapping_neg(),
                    _ => unreachable!(),
                }
            };
            write_reg(cpu, rd, selected, is_64, false)?;
            continue;
        }

        // Unscaled immediate integer loads/stores: LDUR/STUR B/H/W/X.
        // Pre/post-index and signed-load variants are deliberately rejected.
        if instruction & 0x3B20_0000 == 0x3800_0000
            && instruction & 0x0400_0000 == 0
            && instruction & 0x0080_0000 == 0
            && (instruction >> 10) & 0x3 == 0 {
            let size_code = instruction >> 30;
            let width = 1usize << size_code;
            let load = instruction & 0x0040_0000 != 0;
            let immediate = sign_extend(((instruction >> 12) & 0x1FF) as u64, 9);
            let rn = (instruction >> 5) & 0x1F;
            let rt = instruction & 0x1F;
            let base = read_reg(cpu, rn, true, true)?;
            let address = base.checked_add_signed(immediate).ok_or(Arm64Error::AddressOverflow)?;
            if load {
                let value = match width {
                    1 => memory.read_u8(address)? as u64,
                    2 => memory.read_u16(address)? as u64,
                    4 => memory.read_u32(address)? as u64,
                    8 => memory.read_u64(address)?,
                    _ => unreachable!(),
                };
                write_reg(cpu, rt, value, width == 8, false)?;
            } else {
                let value = read_reg(cpu, rt, width == 8, false)?;
                match width {
                    1 => memory.write_u8(address, value as u8)?,
                    2 => memory.write_u16(address, value as u16)?,
                    4 => memory.write_u32(address, value as u32)?,
                    8 => memory.write_u64(address, value)?,
                    _ => unreachable!(),
                }
            }
            continue;
        }

        // Unsigned-offset LDR/STR (integer, 32/64-bit).
        if instruction & 0x3B00_0000 == 0x3900_0000 && instruction & 0x0400_0000 == 0 {
            let size_code = instruction >> 30;
            let width = 1usize << size_code;
            let opc = (instruction >> 22) & 0x3;
            let load = opc != 0;
            let imm = ((instruction >> 10) & 0xFFF) as u64;
            let address_offset = imm.checked_mul(width as u64).ok_or(Arm64Error::AddressOverflow)?;
            let rn = (instruction >> 5) & 0x1F;
            let rt = instruction & 0x1F;
            let base = read_reg(cpu, rn, true, true)?;
            let address = base.checked_add(address_offset).ok_or(Arm64Error::AddressOverflow)?;
            if opc >= 2 {
                if !load || width == 8 {
                    return Err(Arm64Error::UnsupportedInstruction(instruction));
                }
                let signed = match width {
                    1 => memory.read_u8(address)? as i8 as i64 as u64,
                    2 => memory.read_u16(address)? as i16 as i64 as u64,
                    4 => memory.read_u32(address)? as i32 as i64 as u64,
                    _ => unreachable!(),
                };
                write_reg(cpu, rt, signed, opc == 2, false)?;
            } else if load {
                let value = match width {
                    1 => memory.read_u8(address)? as u64,
                    2 => memory.read_u16(address)? as u64,
                    4 => memory.read_u32(address)? as u64,
                    8 => memory.read_u64(address)?,
                    _ => unreachable!(),
                };
                write_reg(cpu, rt, value, width == 8, false)?;
            } else {
                let value = read_reg(cpu, rt, width == 8, false)?;
                match width {
                    1 => memory.write_u8(address, value as u8)?,
                    2 => memory.write_u16(address, value as u16)?,
                    4 => memory.write_u32(address, value as u32)?,
                    8 => memory.write_u64(address, value)?,
                    _ => unreachable!(),
                }
            }
            continue;
        }

        // AArch64 exclusive loads/stores used by compiler-generated atomics.
        // Acquire/release variants share the same single-interpreter ordering
        // here; cross-guest-thread reservation arbitration remains a scheduler task.
        let exclusive_class = instruction & 0x3F20_7C00;
        if exclusive_class == 0x085F_7C00 {
            let size_code = instruction >> 30;
            let width = 1usize << size_code;
            let rn = (instruction >> 5) & 0x1F;
            let rt = instruction & 0x1F;
            let address = read_reg(cpu, rn, true, true)?;
            let value = match width {
                1 => memory.read_u8(address)? as u64,
                2 => memory.read_u16(address)? as u64,
                4 => memory.read_u32(address)? as u64,
                8 => memory.read_u64(address)?,
                _ => unreachable!(),
            };
            write_reg(cpu, rt, value, width == 8, false)?;
            cpu.exclusive_monitor = Some((address, width));
            continue;
        }
        if exclusive_class == 0x0800_7C00 {
            let size_code = instruction >> 30;
            let width = 1usize << size_code;
            let rs = (instruction >> 16) & 0x1F;
            let rn = (instruction >> 5) & 0x1F;
            let rt = instruction & 0x1F;
            let address = read_reg(cpu, rn, true, true)?;
            let value = read_reg(cpu, rt, width == 8, false)?;
            let success = cpu.exclusive_monitor == Some((address, width));
            cpu.exclusive_monitor = None;
            if success {
                match width {
                    1 => memory.write_u8(address, value as u8)?,
                    2 => memory.write_u16(address, value as u16)?,
                    4 => memory.write_u32(address, value as u32)?,
                    8 => memory.write_u64(address, value)?,
                    _ => unreachable!(),
                }
            }
            write_reg(cpu, rs, if success { 0 } else { 1 }, false, false)?;
            continue;
        }

        // MRS TPIDR_EL0 / TPIDRRO_EL0 (Thread ID Register used for Bionic TLS and stack guard)
        if instruction & 0xFFD0_FFE0 == 0xD530_D040 {
            let rt = instruction & 0x1F;
            let op2 = (instruction >> 5) & 0x7;
            let val = match op2 {
                2 => cpu.tpidr_el0,
                3 => cpu.tpidr_el0, // Read-only user TLS alias
                _ => return Err(Arm64Error::UnsupportedInstruction(instruction)),
            };
            write_reg(cpu, rt, val, true, false)?;
            continue;
        }

        // MSR TPIDR_EL0 (Write Thread ID Register)
        if instruction & 0xFFF0_FFE0 == 0xD510_D040 {
            let rt = instruction & 0x1F;
            let val = read_reg(cpu, rt, true, false)?;
            cpu.tpidr_el0 = val;
            continue;
        }

        // Data-processing (3 source): MADD, MSUB, SMADDL, SMSUBL, SMULH, UMADDL, UMSUBL, UMULH
        if instruction & 0x1F00_0000 == 0x1B00_0000 {
            let is_64 = instruction >> 31 != 0;
            let op54 = (instruction >> 29) & 0x3;
            let op31 = (instruction >> 21) & 0x7;
            let rm = (instruction >> 16) & 0x1F;
            let o0 = (instruction >> 15) & 1;
            let ra = (instruction >> 10) & 0x1F;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;

            if op54 != 0 {
                return Err(Arm64Error::UnsupportedInstruction(instruction));
            }

            match (is_64, op31, o0) {
                // MADD (32-bit and 64-bit) including MUL aliases (Ra = 31 / XZR / WZR)
                (true, 0, 0) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, true, false)?;
                    let m = read_reg(cpu, rm, true, false)?;
                    let res = a.wrapping_add(n.wrapping_mul(m));
                    write_reg(cpu, rd, res, true, false)?;
                }
                // MSUB (32-bit and 64-bit) including MNEG aliases
                (true, 0, 1) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, true, false)?;
                    let m = read_reg(cpu, rm, true, false)?;
                    let res = a.wrapping_sub(n.wrapping_mul(m));
                    write_reg(cpu, rd, res, true, false)?;
                }
                (false, 0, 0) => {
                    let a = read_reg(cpu, ra, false, false)? as u32;
                    let n = read_reg(cpu, rn, false, false)? as u32;
                    let m = read_reg(cpu, rm, false, false)? as u32;
                    let res = a.wrapping_add(n.wrapping_mul(m)) as u64;
                    write_reg(cpu, rd, res, false, false)?;
                }
                (false, 0, 1) => {
                    let a = read_reg(cpu, ra, false, false)? as u32;
                    let n = read_reg(cpu, rn, false, false)? as u32;
                    let m = read_reg(cpu, rm, false, false)? as u32;
                    let res = a.wrapping_sub(n.wrapping_mul(m)) as u64;
                    write_reg(cpu, rd, res, false, false)?;
                }
                // SMULH (signed multiply high 64-bit)
                (true, 2, 0) => {
                    let n = read_reg(cpu, rn, true, false)? as i64 as i128;
                    let m = read_reg(cpu, rm, true, false)? as i64 as i128;
                    let prod = n.wrapping_mul(m);
                    let res = (prod >> 64) as u64;
                    write_reg(cpu, rd, res, true, false)?;
                }
                // UMULH (unsigned multiply high 64-bit)
                (true, 6, 0) => {
                    let n = read_reg(cpu, rn, true, false)? as u128;
                    let m = read_reg(cpu, rm, true, false)? as u128;
                    let prod = n.wrapping_mul(m);
                    let res = (prod >> 64) as u64;
                    write_reg(cpu, rd, res, true, false)?;
                }
                // SMADDL / SMSUBL (signed 32x32 -> 64 with accumulate)
                (false, 1, 0) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, false, false)? as u32 as i32 as i64;
                    let m = read_reg(cpu, rm, false, false)? as u32 as i32 as i64;
                    let res = a.wrapping_add(n.wrapping_mul(m) as u64);
                    write_reg(cpu, rd, res, true, false)?;
                }
                (false, 1, 1) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, false, false)? as u32 as i32 as i64;
                    let m = read_reg(cpu, rm, false, false)? as u32 as i32 as i64;
                    let res = a.wrapping_sub(n.wrapping_mul(m) as u64);
                    write_reg(cpu, rd, res, true, false)?;
                }
                // UMADDL / UMSUBL (unsigned 32x32 -> 64 with accumulate)
                (false, 5, 0) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, false, false)? as u32 as u64;
                    let m = read_reg(cpu, rm, false, false)? as u32 as u64;
                    let res = a.wrapping_add(n.wrapping_mul(m));
                    write_reg(cpu, rd, res, true, false)?;
                }
                (false, 5, 1) => {
                    let a = read_reg(cpu, ra, true, false)?;
                    let n = read_reg(cpu, rn, false, false)? as u32 as u64;
                    let m = read_reg(cpu, rm, false, false)? as u32 as u64;
                    let res = a.wrapping_sub(n.wrapping_mul(m));
                    write_reg(cpu, rd, res, true, false)?;
                }
                _ => return Err(Arm64Error::UnsupportedInstruction(instruction)),
            }
            continue;
        }

        // Integer divide: SDIV and UDIV
        if instruction & 0x7FE0_F800 == 0x1AC0_0800 {
            let is_64 = instruction >> 31 != 0;
            let is_sdiv = (instruction >> 10) & 1 != 0;
            let rm = (instruction >> 16) & 0x1F;
            let rn = (instruction >> 5) & 0x1F;
            let rd = instruction & 0x1F;

            let res = if is_64 {
                let n = read_reg(cpu, rn, true, false)?;
                let m = read_reg(cpu, rm, true, false)?;
                if is_sdiv {
                    let sn = n as i64;
                    let sm = m as i64;
                    if sm == 0 {
                        0u64
                    } else if sn == i64::MIN && sm == -1 {
                        i64::MIN as u64
                    } else {
                        (sn / sm) as u64
                    }
                } else {
                    if m == 0 { 0u64 } else { n / m }
                }
            } else {
                let n = read_reg(cpu, rn, false, false)? as u32;
                let m = read_reg(cpu, rm, false, false)? as u32;
                if is_sdiv {
                    let sn = n as i32;
                    let sm = m as i32;
                    if sm == 0 {
                        0u64
                    } else if sn == i32::MIN && sm == -1 {
                        i32::MIN as u32 as u64
                    } else {
                        (sn / sm) as u32 as u64
                    }
                } else {
                    if m == 0 { 0u64 } else { (n / m) as u64 }
                }
            };
            write_reg(cpu, rd, res, is_64, false)?;
            continue;
        }

        // FP / SIMD unsigned-offset LDR/STR (32-bit single, 64-bit double, 128-bit quad)
        if instruction & 0x3B80_0000 == 0x3D80_0000 && instruction & 0x0020_0000 == 0 {
            let size_code = instruction >> 30;
            let opc = (instruction >> 22) & 0x3;
            let load = opc != 0;
            let width = match (size_code, opc) {
                (0, 0) | (0, 1) => 1usize, // B
                (1, 0) | (1, 1) => 2usize, // H
                (2, 0) | (2, 1) => 4usize, // S
                (3, 0) | (3, 1) => 8usize, // D
                (0, 2) | (0, 3) => 16usize, // Q
                _ => return Err(Arm64Error::UnsupportedInstruction(instruction)),
            };
            let imm = ((instruction >> 10) & 0xFFF) as u64;
            let address_offset = imm.checked_mul(width as u64).ok_or(Arm64Error::AddressOverflow)?;
            let rn = (instruction >> 5) & 0x1F;
            let rt = (instruction & 0x1F) as usize;
            let base = read_reg(cpu, rn, true, true)?;
            let address = base.checked_add(address_offset).ok_or(Arm64Error::AddressOverflow)?;

            if load {
                let val: u128 = match width {
                    1 => memory.read_u8(address)? as u128,
                    2 => memory.read_u16(address)? as u128,
                    4 => memory.read_u32(address)? as u128,
                    8 => memory.read_u64(address)? as u128,
                    16 => {
                        let lo = memory.read_u64(address)? as u128;
                        let hi = memory.read_u64(address.checked_add(8).ok_or(Arm64Error::AddressOverflow)?)? as u128;
                        (hi << 64) | lo
                    }
                    _ => unreachable!(),
                };
                cpu.v[rt] = val;
            } else {
                let val = cpu.v[rt];
                match width {
                    1 => memory.write_u8(address, val as u8)?,
                    2 => memory.write_u16(address, val as u16)?,
                    4 => memory.write_u32(address, val as u32)?,
                    8 => memory.write_u64(address, val as u64)?,
                    16 => {
                        memory.write_u64(address, val as u64)?;
                        memory.write_u64(address.checked_add(8).ok_or(Arm64Error::AddressOverflow)?, (val >> 64) as u64)?;
                    }
                    _ => unreachable!(),
                }
            }
            continue;
        }

        // FMOV between integer and FP registers
        if instruction & 0xFFFF_FC00 == 0x1E26_0000 { // FMOV Wd, Sn
            let rn = ((instruction >> 5) & 0x1F) as usize;
            let rd = instruction & 0x1F;
            write_reg(cpu, rd, (cpu.v[rn] as u32) as u64, false, false)?;
            continue;
        }
        if instruction & 0xFFFF_FC00 == 0x1E27_0000 { // FMOV Sn, Wd
            let rn = (instruction >> 5) & 0x1F;
            let rd = (instruction & 0x1F) as usize;
            let val = read_reg(cpu, rn, false, false)? as u32;
            cpu.v[rd] = val as u128;
            continue;
        }
        if instruction & 0xFFFF_FC00 == 0x9E66_0000 { // FMOV Xd, Dn
            let rn = ((instruction >> 5) & 0x1F) as usize;
            let rd = instruction & 0x1F;
            write_reg(cpu, rd, cpu.v[rn] as u64, true, false)?;
            continue;
        }
        if instruction & 0xFFFF_FC00 == 0x9E67_0000 { // FMOV Dn, Xd
            let rn = (instruction >> 5) & 0x1F;
            let rd = (instruction & 0x1F) as usize;
            let val = read_reg(cpu, rn, true, false)?;
            cpu.v[rd] = val as u128;
            continue;
        }
        // BR, BLR and RET.
        let branch_kind = instruction & 0xFFFF_FC1F;
        if branch_kind == 0xD61F_0000 || branch_kind == 0xD63F_0000 || branch_kind == 0xD65F_0000 {
            let rn = (instruction >> 5) & 0x1F;
            if rn >= 31 { return Err(Arm64Error::InvalidRegister); }
            let target = cpu.x[rn as usize];
            if target & 3 != 0 { return Err(Arm64Error::InvalidBranchTarget); }
            if branch_kind == 0xD63F_0000 { cpu.x[30] = next_pc; }
            if branch_kind == 0xD65F_0000 && target == 0 {
                cpu.halted = true;
                return Ok(Arm64RunReport { stop_reason: Arm64StopReason::Halted, instructions_executed: executed_this_run, final_pc: cpu.pc });
            }
            cpu.pc = target;
            continue;
        }

        return Err(Arm64Error::UnsupportedInstruction(instruction));
    }

    Ok(Arm64RunReport {
        stop_reason: Arm64StopReason::InstructionBudget,
        instructions_executed: executed_this_run,
        final_pc: cpu.pc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(base: u64, code: &[u32]) -> GuestMemory {
        let bytes = code.iter().flat_map(|word| word.to_le_bytes()).collect();
        GuestMemory::from_bytes(base, bytes).unwrap()
    }

    #[test]
    fn decodes_replicated_logical_immediate_masks() {
        assert_eq!(decode_logical_immediate(false, 0, 0, 7), Some(0xFF));
        assert_eq!(decode_logical_immediate(true, 1, 0, 7), Some(0xFF));
        assert_eq!(decode_logical_immediate(true, 1, 8, 7), Some(0xFF00_0000_0000_0000));
        assert_eq!(decode_logical_immediate(true, 1, 0, 63), None);
        assert_eq!(decode_logical_immediate(false, 1, 0, 7), None);
    }

    #[test]
    fn executes_movz_add_and_return() {
        // MOVZ X0, #7; ADD X0, X0, #5; RET X30 (zero halts).
        let mut memory = words(0x1000, &[0xD28000E0, 0x91001400, 0xD65F03C0]);
        let mut cpu = Arm64Cpu::default();
        let report = run_aarch64(&mut cpu, &mut memory, 0x1000, 8).unwrap();
        assert_eq!(cpu.x[0], 12);
        assert_eq!(report.stop_reason, Arm64StopReason::Halted);
        assert_eq!(report.instructions_executed, 3);
    }

    #[test]
    fn enforces_instruction_budget() {
        let mut memory = words(0x2000, &[0xD503201F, 0x14000000]);
        let mut cpu = Arm64Cpu::default();
        let report = run_aarch64(&mut cpu, &mut memory, 0x2000, 3).unwrap();
        assert_eq!(report.stop_reason, Arm64StopReason::InstructionBudget);
        assert_eq!(report.instructions_executed, 3);
    }

    #[test]
    fn rejects_unmapped_load_store() {
        let mut memory = words(0x3000, &[0xF9400020]);
        let mut cpu = Arm64Cpu::default();
        cpu.x[1] = 0x9000;
        assert_eq!(run_aarch64(&mut cpu, &mut memory, 0x3000, 1), Err(Arm64Error::MemoryOutOfBounds));
    }

    #[test]
    fn rejects_unknown_instruction() {
        let mut memory = words(0x4000, &[0xFFFFFFFF]);
        let mut cpu = Arm64Cpu::default();
        assert_eq!(run_aarch64(&mut cpu, &mut memory, 0x4000, 1), Err(Arm64Error::UnsupportedInstruction(0xFFFFFFFF)));
    }
    #[test]
    fn rejects_data_fetch_from_non_executable_guest_page() {
        let mut memory = words(0x5000, &[0xD503201F]);
        memory.enable_strict_protections();
        memory.map_protection(0x5000, 4, MemoryProtection { readable: true, writable: false, executable: false }).unwrap();
        let mut cpu = Arm64Cpu::default();
        assert_eq!(run_aarch64(&mut cpu, &mut memory, 0x5000, 1), Err(Arm64Error::MemoryOutOfBounds));
    }

    #[test]
    fn rejects_writes_to_read_only_guest_page() {
        let mut memory = words(0x6000, &[0xD503201F]);
        memory.enable_strict_protections();
        memory.map_protection(0x6000, 4, MemoryProtection { readable: true, writable: false, executable: true }).unwrap();
        assert_eq!(memory.write_u32(0x6000, 0), Err(Arm64Error::MemoryOutOfBounds));
    }

    #[test]
    fn enforces_global_instruction_budget_ceiling() {
        let mut memory = words(0x7000, &[0xD503201F]);
        let mut cpu = Arm64Cpu::default();
        assert_eq!(run_aarch64(&mut cpu, &mut memory, 0x7000, MAX_INSTRUCTIONS_PER_RUN + 1), Err(Arm64Error::InstructionLimit));
    }
    #[test]
    fn executes_mrs_msr_tpidr_el0() {
        // msr tpidr_el0, x0 (0xD513D040)
        // mrs x1, tpidr_el0 (0xD533D041)
        // ret (0xD65F03C0)
        let mut memory = words(0x1000, &[0xD513D040, 0xD533D041, 0xD65F03C0]);
        let mut cpu = Arm64Cpu::default();
        cpu.x[0] = 0xDEAD_BEEF_CAFE_BABE;
        cpu.x[30] = 0;
        let report = run_aarch64(&mut cpu, &mut memory, 0x1000, 10).unwrap();
        assert_eq!(report.stop_reason, Arm64StopReason::Halted);
        assert_eq!(cpu.tpidr_el0, 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(cpu.x[1], 0xDEAD_BEEF_CAFE_BABE);
    }

    #[test]
    fn executes_madd_and_msub() {
        // madd x0, x1, x2, x3 -> x0 = x3 + (x1 * x2)
        // mul x4, x1, x2 -> x4 = xzr + (x1 * x2) = x1 * x2
        // msub x5, x1, x2, x3 -> x5 = x3 - (x1 * x2)
        // ret
        // madd: 0x9B020C20 (rm=2, ra=3, rn=1, rd=0)
        // mul:  0x9B027C24 (rm=2, ra=31, rn=1, rd=4)
        // msub: 0x9B028C25 (rm=2, ra=3, rn=1, rd=5)
        let mut memory = words(0x1000, &[0x9B020C20, 0x9B027C24, 0x9B028C25, 0xD65F03C0]);
        let mut cpu = Arm64Cpu::default();
        cpu.x[1] = 6;
        cpu.x[2] = 7;
        cpu.x[3] = 10;
        cpu.x[30] = 0;
        let report = run_aarch64(&mut cpu, &mut memory, 0x1000, 10).unwrap();
        assert_eq!(report.stop_reason, Arm64StopReason::Halted);
        assert_eq!(cpu.x[0], 52); // 10 + 42
        assert_eq!(cpu.x[4], 42); // 6 * 7
        assert_eq!(cpu.x[5], 10u64.wrapping_sub(42)); // 10 - 42
    }

    #[test]
    fn executes_sdiv_and_udiv() {
        // sdiv x0, x1, x2 (0x9AC20C20)
        // udiv x3, x4, x5 (0x9AC50883)
        // ret
        let mut memory = words(0x1000, &[0x9AC20C20, 0x9AC50883, 0xD65F03C0]);
        let mut cpu = Arm64Cpu::default();
        cpu.x[1] = (-42i64) as u64;
        cpu.x[2] = 6;
        cpu.x[4] = 100;
        cpu.x[5] = 5;
        cpu.x[30] = 0;
        let report = run_aarch64(&mut cpu, &mut memory, 0x1000, 10).unwrap();
        assert_eq!(report.stop_reason, Arm64StopReason::Halted);
        assert_eq!(cpu.x[0] as i64, -7);
        assert_eq!(cpu.x[3], 20);
    }
}
