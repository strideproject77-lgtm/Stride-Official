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
        Ok(Self { base, bytes })
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
    pub fn write_u32(&mut self, address: u64, value: u32) -> Result<(), Arm64Error> {
        self.check_permission(address, 4, false, true)?;
        let r = self.range(address, 4)?;
        self.bytes[r].copy_from_slice(&value.to_le_bytes());
        Ok(())
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
    let start_count = cpu.instructions_executed;

    while cpu.instructions_executed - start_count < instruction_budget {
        if cpu.pc & 3 != 0 { return Err(Arm64Error::MisalignedProgramCounter); }
        let instruction = memory.fetch_instruction(cpu.pc)?;
        let current_pc = cpu.pc;
        let next_pc = current_pc.checked_add(4).ok_or(Arm64Error::AddressOverflow)?;
        cpu.pc = next_pc;
        cpu.instructions_executed = cpu.instructions_executed.saturating_add(1);

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
        // Unsigned-offset LDR/STR (integer, 32/64-bit).
        if instruction & 0x3B00_0000 == 0x3900_0000 && instruction & 0x0400_0000 == 0 {
            let size_code = instruction >> 30;
            if size_code != 2 && size_code != 3 {
                return Err(Arm64Error::UnsupportedInstruction(instruction));
            }
            let width = 1usize << size_code;
            let load = instruction & 0x0040_0000 != 0;
            let imm = ((instruction >> 10) & 0xFFF) as u64;
            let address_offset = imm.checked_mul(width as u64).ok_or(Arm64Error::AddressOverflow)?;
            let rn = (instruction >> 5) & 0x1F;
            let rt = instruction & 0x1F;
            let base = read_reg(cpu, rn, true, true)?;
            let address = base.checked_add(address_offset).ok_or(Arm64Error::AddressOverflow)?;
            if load {
                let value = if width == 8 { memory.read_u64(address)? } else { memory.read_u32(address)? as u64 };
                write_reg(cpu, rt, value, width == 8, false)?;
            } else {
                let value = read_reg(cpu, rt, width == 8, false)?;
                if width == 8 { memory.write_u64(address, value)?; }
                else { memory.write_u32(address, value as u32)?; }
            }
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
                return Ok(Arm64RunReport { stop_reason: Arm64StopReason::Halted, instructions_executed: cpu.instructions_executed - start_count, final_pc: cpu.pc });
            }
            cpu.pc = target;
            continue;
        }

        return Err(Arm64Error::UnsupportedInstruction(instruction));
    }

    Ok(Arm64RunReport {
        stop_reason: Arm64StopReason::InstructionBudget,
        instructions_executed: cpu.instructions_executed - start_count,
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
}
