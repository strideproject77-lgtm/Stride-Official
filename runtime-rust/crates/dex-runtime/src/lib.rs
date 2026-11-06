//! A deliberately small, bounds-checked Dalvik integer interpreter.
//!
//! This is an execution primitive, not ART: it supports a documented subset of
//! register moves, integer constants, integer arithmetic, branches, and returns.
//! It does not load APKs, resolve classes, perform JNI, or execute native code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnValue {
    Void,
    Int(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionReport {
    pub value: ReturnValue,
    pub steps: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    EmptyMethod,
    InvalidRegister(u16),
    TruncatedInstruction,
    UnsupportedOpcode(u8),
    InvalidBranchTarget,
    DivisionByZero,
    StepLimitExceeded,
}

fn width(op: u8) -> Option<usize> {
    match op {
        0x00 | 0x01 | 0x0e | 0x0f | 0x12 | 0x28 | 0xb0..=0xb4 => Some(1),
        0x02 | 0x13 | 0x15 | 0x29 | 0x32..=0x3d | 0x90..=0x94 => Some(2),
        0x03 | 0x14 | 0x2a => Some(3),
        _ => None,
    }
}

fn register(registers: &[i32], index: u16) -> Result<i32, VmError> {
    registers.get(index as usize).copied().ok_or(VmError::InvalidRegister(index))
}

fn write_register(registers: &mut [i32], index: u16, value: i32) -> Result<(), VmError> {
    let slot = registers.get_mut(index as usize).ok_or(VmError::InvalidRegister(index))?;
    *slot = value;
    Ok(())
}

fn branch_target(
    source: usize,
    delta: isize,
    boundaries: &[bool],
) -> Result<usize, VmError> {
    let target = (source as isize)
        .checked_add(delta)
        .ok_or(VmError::InvalidBranchTarget)?;
    if target < 0 || target as usize >= boundaries.len() || !boundaries[target as usize] {
        return Err(VmError::InvalidBranchTarget);
    }
    Ok(target as usize)
}

/// Executes one already-selected DEX method body represented as little-endian code units.
///
/// `initial_registers` fills the lowest-numbered registers. All remaining registers start
/// at zero. The caller is responsible for selecting a method and preparing its arguments.
pub fn execute_integer_method(
    code_units: &[u16],
    register_count: usize,
    initial_registers: &[i32],
    step_limit: usize,
) -> Result<ExecutionReport, VmError> {
    if code_units.is_empty() {
        return Err(VmError::EmptyMethod);
    }
    if initial_registers.len() > register_count {
        return Err(VmError::InvalidRegister(register_count as u16));
    }

    // Preflight instruction boundaries so branches cannot land inside operands.
    let mut boundaries = vec![false; code_units.len() + 1];
    let mut scan = 0usize;
    while scan < code_units.len() {
        boundaries[scan] = true;
        let op = (code_units[scan] & 0xff) as u8;
        let instruction_width = width(op).ok_or(VmError::UnsupportedOpcode(op))?;
        if instruction_width > code_units.len() - scan {
            return Err(VmError::TruncatedInstruction);
        }
        scan += instruction_width;
    }
    boundaries[code_units.len()] = true;

    let mut registers = vec![0i32; register_count];
    registers[..initial_registers.len()].copy_from_slice(initial_registers);
    let mut pc = 0usize;
    let mut steps = 0usize;

    loop {
        if steps >= step_limit {
            return Err(VmError::StepLimitExceeded);
        }
        if pc >= code_units.len() || !boundaries[pc] {
            return Err(VmError::InvalidBranchTarget);
        }
        steps += 1;

        let first = code_units[pc];
        let op = (first & 0xff) as u8;
        let a8 = (first >> 8) as u16;
        let a4 = ((first >> 8) & 0x0f) as u16;
        let b4 = (first >> 12) as u16;

        match op {
            0x00 => pc += 1, // nop
            0x01 => { // move vA, vB (12x)
                let value = register(&registers, b4)?;
                write_register(&mut registers, a4, value)?;
                pc += 1;
            }
            0x02 => { // move/from16 vAA, vBBBB
                let b = code_units[pc + 1];
                let value = register(&registers, b)?;
                write_register(&mut registers, a8, value)?;
                pc += 2;
            }
            0x03 => { // move/16 vAAAA, vBBBB
                let a = code_units[pc + 1];
                let b = code_units[pc + 2];
                let value = register(&registers, b)?;
                write_register(&mut registers, a, value)?;
                pc += 3;
            }
            0x0e => return Ok(ExecutionReport { value: ReturnValue::Void, steps }),
            0x0f => {
                return Ok(ExecutionReport {
                    value: ReturnValue::Int(register(&registers, a8)?),
                    steps,
                });
            }
            0x12 => { // const/4 vA, #+B (11n)
                let literal = ((first >> 12) as i16) << 12 >> 12;
                write_register(&mut registers, a4, literal as i32)?;
                pc += 1;
            }
            0x13 => { // const/16 vAA, #+BBBB
                write_register(&mut registers, a8, code_units[pc + 1] as i16 as i32)?;
                pc += 2;
            }
            0x14 => { // const vAA, #+BBBBBBBB
                let bits = (code_units[pc + 1] as u32)
                    | ((code_units[pc + 2] as u32) << 16);
                write_register(&mut registers, a8, bits as i32)?;
                pc += 3;
            }
            0x15 => { // const/high16 vAA, #+BBBB0000
                let value = (code_units[pc + 1] as i16 as i32) << 16;
                write_register(&mut registers, a8, value)?;
                pc += 2;
            }
            0x28 => { // goto +AA
                let delta = (first >> 8) as u8 as i8 as isize;
                pc = branch_target(pc, delta, &boundaries)?;
            }
            0x29 => { // goto/16 +AAAA
                let delta = code_units[pc + 1] as i16 as isize;
                pc = branch_target(pc, delta, &boundaries)?;
            }
            0x2a => { // goto/32 +AAAAAAAA
                let bits = (code_units[pc + 1] as u32)
                    | ((code_units[pc + 2] as u32) << 16);
                pc = branch_target(pc, bits as i32 as isize, &boundaries)?;
            }
            0x32..=0x37 => { // if-* vA, vB, +CCCC
                let lhs = register(&registers, a4)?;
                let rhs = register(&registers, b4)?;
                let delta = code_units[pc + 1] as i16 as isize;
                let take = match op {
                    0x32 => lhs == rhs,
                    0x33 => lhs != rhs,
                    0x34 => lhs < rhs,
                    0x35 => lhs >= rhs,
                    0x36 => lhs > rhs,
                    _ => lhs <= rhs,
                };
                pc = if take { branch_target(pc, delta, &boundaries)? } else { pc + 2 };
            }
            0x38..=0x3d => { // if-*z vAA, +BBBB
                let value = register(&registers, a8)?;
                let delta = code_units[pc + 1] as i16 as isize;
                let take = match op {
                    0x38 => value == 0,
                    0x39 => value != 0,
                    0x3a => value < 0,
                    0x3b => value >= 0,
                    0x3c => value > 0,
                    _ => value <= 0,
                };
                pc = if take { branch_target(pc, delta, &boundaries)? } else { pc + 2 };
            }
            0x90..=0x94 => { // add/sub/mul/div/rem-int vAA, vBB, vCC
                let second = code_units[pc + 1];
                let b = (second & 0xff) as u16;
                let c = (second >> 8) as u16;
                let lhs = register(&registers, b)?;
                let rhs = register(&registers, c)?;
                let value = match op {
                    0x90 => lhs.wrapping_add(rhs),
                    0x91 => lhs.wrapping_sub(rhs),
                    0x92 => lhs.wrapping_mul(rhs),
                    0x93 => {
                        if rhs == 0 { return Err(VmError::DivisionByZero); }
                        lhs.checked_div(rhs).unwrap_or(i32::MIN)
                    }
                    _ => {
                        if rhs == 0 { return Err(VmError::DivisionByZero); }
                        lhs.checked_rem(rhs).unwrap_or(0)
                    }
                };
                write_register(&mut registers, a8, value)?;
                pc += 2;
            }
            0xb0..=0xb4 => { // *-int/2addr vA, vB
                let lhs = register(&registers, a4)?;
                let rhs = register(&registers, b4)?;
                let value = match op {
                    0xb0 => lhs.wrapping_add(rhs),
                    0xb1 => lhs.wrapping_sub(rhs),
                    0xb2 => lhs.wrapping_mul(rhs),
                    0xb3 => {
                        if rhs == 0 { return Err(VmError::DivisionByZero); }
                        lhs.checked_div(rhs).unwrap_or(i32::MIN)
                    }
                    _ => {
                        if rhs == 0 { return Err(VmError::DivisionByZero); }
                        lhs.checked_rem(rhs).unwrap_or(0)
                    }
                };
                write_register(&mut registers, a4, value)?;
                pc += 1;
            }
            _ => return Err(VmError::UnsupportedOpcode(op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_constant_add_and_return() {
        // const/4 v0, 7; const/4 v1, 5; add-int v2, v0, v1; return v2
        let code = [0x7012, 0x5012, 0x0090, 0x0200, 0x020f];
        let result = execute_integer_method(&code, 3, &[], 20).unwrap();
        assert_eq!(result.value, ReturnValue::Int(12));
        assert_eq!(result.steps, 4);
    }

    #[test]
    fn executes_conditional_branch() {
        // const/4 v0, 0; if-eqz v0, +3; const/4 v1, 1; return v1;
        // branch skips const and returns v1=0.
        let code = [0x0012, 0x0038, 0x0003, 0x1012, 0x010f];
        let result = execute_integer_method(&code, 2, &[], 20).unwrap();
        assert_eq!(result.value, ReturnValue::Int(0));
    }

    #[test]
    fn rejects_invalid_registers() {
        let code = [0x0012, 0x000f];
        assert_eq!(
            execute_integer_method(&code, 0, &[], 10),
            Err(VmError::InvalidRegister(0))
        );
    }

    #[test]
    fn rejects_division_by_zero() {
        let code = [0x0012, 0x0012, 0x0093, 0x0000, 0x000f];
        assert_eq!(
            execute_integer_method(&code, 1, &[], 10),
            Err(VmError::DivisionByZero)
        );
    }

    #[test]
    fn enforces_instruction_budget_on_loops() {
        let code = [0x0028]; // goto itself
        assert_eq!(
            execute_integer_method(&code, 0, &[], 4),
            Err(VmError::StepLimitExceeded)
        );
    }

    #[test]
    fn rejects_branch_into_operand() {
        let code = [0x0128, 0x000e]; // goto +1 points at return's code unit
        assert_eq!(
            execute_integer_method(&code, 0, &[], 10),
            Err(VmError::InvalidBranchTarget)
        );
    }

    #[test]
    fn rejects_unsupported_object_and_invoke_opcodes() {
        let code = [0x006e, 0x0000, 0x0000];
        assert_eq!(
            execute_integer_method(&code, 1, &[], 10),
            Err(VmError::UnsupportedOpcode(0x6e))
        );
    }
}
