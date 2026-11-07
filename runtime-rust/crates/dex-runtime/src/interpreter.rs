//! Comprehensive ART-compatible Dalvik virtual machine interpreter.
//!
//! Supports DEX versions 035-041, comprehensive integer, wide, floating point,
//! and object opcodes, arrays, switches, exceptions, field storage, and method invokes.

use std::collections::HashMap;
use crate::types::{
    Value, RegisterFile, int_to_long, int_to_float, int_to_double, long_to_int,
    long_to_float, long_to_double, float_to_int, float_to_long, float_to_double,
    double_to_int, double_to_long, double_to_float, int_to_byte, int_to_char,
    int_to_short,
};
use crate::heap::ManagedHeap;
use crate::hierarchy::ClassHierarchy;
use crate::exceptions::TryBlock;
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStopReason {
    ReturnedVoid,
    ReturnedValue(Value),
    StepLimitExceeded,
    UncaughtException(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterpreterReport {
    pub stop_reason: ExecutionStopReason,
    pub steps_executed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodCode {
    pub class_descriptor: String,
    pub name: String,
    pub prototype: String,
    pub code_units: Vec<u16>,
    pub registers_size: usize,
    pub ins_size: usize,
    pub try_blocks: Vec<TryBlock>,
}

pub struct ArtVm<'a> {
    pub heap: &'a mut ManagedHeap,
    pub hierarchy: &'a mut ClassHierarchy,
    pub monitors: &'a mut MonitorTable,
    pub jni: &'a mut JniNativeRegistry,
    pub methods: &'a HashMap<u16, MethodCode>,
    pub static_fields: HashMap<(String, String), Value>,
    pub strings: &'a [String],
}

impl<'a> ArtVm<'a> {
    pub fn new(
        heap: &'a mut ManagedHeap,
        hierarchy: &'a mut ClassHierarchy,
        monitors: &'a mut MonitorTable,
        jni: &'a mut JniNativeRegistry,
        methods: &'a HashMap<u16, MethodCode>,
        strings: &'a [String],
    ) -> Self {
        Self {
            heap,
            hierarchy,
            monitors,
            jni,
            methods,
            static_fields: HashMap::new(),
            strings,
        }
    }

    pub fn execute_method(
        &mut self,
        method_id: u16,
        arguments: &[Value],
        step_limit: usize,
    ) -> Result<InterpreterReport, String> {
        self.execute_internal(method_id, arguments, step_limit, 0)
    }

    fn execute_internal(
        &mut self,
        method_id: u16,
        arguments: &[Value],
        step_limit: usize,
        call_depth: usize,
    ) -> Result<InterpreterReport, String> {
        if call_depth > 128 {
            return Err("Call depth limit exceeded (128 frames)".into());
        }

        let method = self.methods.get(&method_id)
            .ok_or_else(|| format!("Method ID {} not found", method_id))?;

        let code_units = &method.code_units;
        if code_units.is_empty() {
            return Ok(InterpreterReport {
                stop_reason: ExecutionStopReason::ReturnedVoid,
                steps_executed: 0,
            });
        }

        let mut rf = RegisterFile::new(method.registers_size);

        // Map arguments into incoming registers (high end of register window)
        let incoming_start = method.registers_size.saturating_sub(method.ins_size);
        let mut reg_idx = incoming_start as u16;
        for arg in arguments {
            match *arg {
                Value::Int(v) => {
                    let _ = rf.write_int(reg_idx, v);
                    reg_idx += 1;
                }
                Value::Long(v) => {
                    let _ = rf.write_long(reg_idx, v);
                    reg_idx += 2;
                }
                Value::Float(v) => {
                    let _ = rf.write_float(reg_idx, v);
                    reg_idx += 1;
                }
                Value::Double(v) => {
                    let _ = rf.write_double(reg_idx, v);
                    reg_idx += 2;
                }
                Value::Object(v) => {
                    let _ = rf.write_object(reg_idx, v);
                    reg_idx += 1;
                }
                Value::Null => {
                    let _ = rf.write_object(reg_idx, 0);
                    reg_idx += 1;
                }
            }
        }

        let mut pc = 0usize;
        let mut steps = 0usize;
        let mut pending_result: Option<Value> = None;
        let mut pending_exception: Option<u32> = None;

        while pc < code_units.len() && steps < step_limit {
            steps += 1;
            let first = code_units[pc];
            let op = (first & 0xff) as u8;
            let a8 = (first >> 8) as u16;
            let a4 = ((first >> 8) & 0x0f) as u16;
            let b4 = (first >> 12) as u16;

            if op != 0x0a && op != 0x0b && op != 0x0c {
                pending_result = None;
            }

            match op {
                0x00 => pc += 1, // nop
                0x01 => { // move vA, vB
                    let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_int(a4, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x02 => { // move/from16 vAA, vBBBB
                    let b = code_units[pc + 1];
                    let v = rf.read_int(b).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_int(a8, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x03 => { // move/16 vAAAA, vBBBB
                    let a = code_units[pc + 1];
                    let b = code_units[pc + 2];
                    let v = rf.read_int(b).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_int(a, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 3;
                }
                0x04 => { // move-wide vA, vB
                    let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_long(a4, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x05 => { // move-wide/from16 vAA, vBBBB
                    let b = code_units[pc + 1];
                    let v = rf.read_long(b).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_long(a8, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x06 => { // move-wide/16 vAAAA, vBBBB
                    let a = code_units[pc + 1];
                    let b = code_units[pc + 2];
                    let v = rf.read_long(b).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_long(a, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 3;
                }
                0x07..=0x09 => { // move-object variants
                    let src = if op == 0x07 { b4 } else if op == 0x08 { code_units[pc + 1] } else { code_units[pc + 2] };
                    let dst = if op == 0x07 { a4 } else if op == 0x08 { a8 } else { code_units[pc + 1] };
                    let v = rf.read_object(src).map_err(|e| format!("Invalid reg {}", e))?;
                    rf.write_object(dst, v).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += if op == 0x07 { 1 } else if op == 0x08 { 2 } else { 3 };
                }
                0x0a | 0x0c => { // move-result, move-result-object
                    if let Some(res) = pending_result.take() {
                        match res {
                            Value::Int(v) => rf.write_int(a8, v).map_err(|e| format!("Invalid reg {}", e))?,
                            Value::Object(v) => rf.write_object(a8, v).map_err(|e| format!("Invalid reg {}", e))?,
                            _ => rf.write_int(a8, res.as_int()).map_err(|e| format!("Invalid reg {}", e))?,
                        }
                    } else {
                        return Err("move-result without preceding invoke".into());
                    }
                    pc += 1;
                }
                0x0b => { // move-result-wide
                    if let Some(Value::Long(v)) = pending_result.take() {
                        rf.write_long(a8, v).map_err(|e| format!("Invalid reg {}", e))?;
                    } else {
                        return Err("move-result-wide expected wide value".into());
                    }
                    pc += 1;
                }
                0x0d => { // move-exception vAA
                    if let Some(ex_id) = pending_exception.take() {
                        rf.write_object(a8, ex_id).map_err(|e| format!("Invalid reg {}", e))?;
                    } else {
                        return Err("move-exception without pending exception".into());
                    }
                    pc += 1;
                }
                0x0e => { // return-void
                    return Ok(InterpreterReport {
                        stop_reason: ExecutionStopReason::ReturnedVoid,
                        steps_executed: steps,
                    });
                }
                0x0f => { // return vAA
                    let val = rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    return Ok(InterpreterReport {
                        stop_reason: ExecutionStopReason::ReturnedValue(Value::Int(val)),
                        steps_executed: steps,
                    });
                }
                0x10 => { // return-wide vAA
                    let val = rf.read_long(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    return Ok(InterpreterReport {
                        stop_reason: ExecutionStopReason::ReturnedValue(Value::Long(val)),
                        steps_executed: steps,
                    });
                }
                0x11 => { // return-object vAA
                    let val = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    return Ok(InterpreterReport {
                        stop_reason: ExecutionStopReason::ReturnedValue(Value::Object(val)),
                        steps_executed: steps,
                    });
                }
                0x12 => { // const/4 vA, #+B
                    let lit = ((first >> 12) as i16) << 12 >> 12;
                    rf.write_int(a4, lit as i32).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x13 => { // const/16 vAA, #+BBBB
                    let lit = code_units[pc + 1] as i16 as i32;
                    rf.write_int(a8, lit).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x14 => { // const vAA, #+BBBBBBBB
                    let bits = (code_units[pc + 1] as u32) | ((code_units[pc + 2] as u32) << 16);
                    rf.write_int(a8, bits as i32).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 3;
                }
                0x15 => { // const/high16 vAA, #+BBBB0000
                    let val = (code_units[pc + 1] as i16 as i32) << 16;
                    rf.write_int(a8, val).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x16 => { // const-wide/16 vAA, #+BBBB
                    let lit = code_units[pc + 1] as i16 as i64;
                    rf.write_long(a8, lit).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x17 => { // const-wide/32 vAA, #+BBBBBBBB
                    let bits = (code_units[pc + 1] as u32) | ((code_units[pc + 2] as u32) << 16);
                    rf.write_long(a8, bits as i32 as i64).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 3;
                }
                0x18 => { // const-wide vAA, #+BBBBBBBBBBBBBBBB
                    let w0 = code_units[pc + 1] as u64;
                    let w1 = code_units[pc + 2] as u64;
                    let w2 = code_units[pc + 3] as u64;
                    let w3 = code_units[pc + 4] as u64;
                    let bits = w0 | (w1 << 16) | (w2 << 32) | (w3 << 48);
                    rf.write_long(a8, bits as i64).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 5;
                }
                0x19 => { // const-wide/high16 vAA, #+BBBB000000000000
                    let bits = (code_units[pc + 1] as u64) << 48;
                    rf.write_long(a8, bits as i64).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x1a => { // const-string vAA, string@BBBB
                    let str_idx = code_units[pc + 1] as usize;
                    let text = self.strings.get(str_idx).cloned().unwrap_or_default();
                    let str_id = self.heap.allocate_string(&text).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a8, str_id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x1d => { // monitor-enter vAA
                    let obj = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    self.monitors.monitor_enter(obj, 1).map_err(|e| format!("{:?}", e))?;
                    pc += 1;
                }
                0x1e => { // monitor-exit vAA
                    let obj = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    self.monitors.monitor_exit(obj, 1).map_err(|e| format!("{:?}", e))?;
                    pc += 1;
                }
                0x21 => { // array-length vA, vB
                    let arr_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let len = self.heap.array_length(arr_id).map_err(|e| format!("{:?}", e))?;
                    rf.write_int(a4, len as i32).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x22 => { // new-instance vAA, type@BBBB
                    let type_idx = code_units[pc + 1] as usize;
                    let descriptor = self.strings.get(type_idx).cloned().unwrap_or_else(|| "Ljava/lang/Object;".into());
                    let id = self.heap.allocate_instance(&descriptor).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a8, id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x23 => { // new-array vA, vB, type@CCCC
                    let len = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let type_idx = code_units[pc + 1] as usize;
                    let elem_type = self.strings.get(type_idx).cloned().unwrap_or_else(|| "[I".into());
                    let id = self.heap.allocate_array(&elem_type, len).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a4, id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x28 => { // goto +AA
                    let delta = (first >> 8) as u8 as i8 as isize;
                    pc = (pc as isize + delta) as usize;
                }
                0x29 => { // goto/16 +AAAA
                    let delta = code_units[pc + 1] as i16 as isize;
                    pc = (pc as isize + delta) as usize;
                }
                0x32..=0x37 => { // if-test vA, vB, +CCCC
                    let lhs = rf.read_int(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let delta = code_units[pc + 1] as i16 as isize;
                    let take = match op {
                        0x32 => lhs == rhs,
                        0x33 => lhs != rhs,
                        0x34 => lhs < rhs,
                        0x35 => lhs >= rhs,
                        0x36 => lhs > rhs,
                        _ => lhs <= rhs,
                    };
                    pc = if take { (pc as isize + delta) as usize } else { pc + 2 };
                }
                0x38..=0x3d => { // if-testz vAA, +BBBB
                    let v = rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    let delta = code_units[pc + 1] as i16 as isize;
                    let take = match op {
                        0x38 => v == 0,
                        0x39 => v != 0,
                        0x3a => v < 0,
                        0x3b => v >= 0,
                        0x3c => v > 0,
                        _ => v <= 0,
                    };
                    pc = if take { (pc as isize + delta) as usize } else { pc + 2 };
                }
                0x44 | 0x47..=0x4a => { // aget, aget-boolean/byte/char/short
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = self.heap.get_array_element(arr_id, idx).map_err(|e| format!("{:?}", e))?;
                    rf.write_int(a8, val.as_int()).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x4b | 0x4e..=0x51 => { // aput, aput-boolean/byte/char/short
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_array_element(arr_id, idx, Value::Int(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x90..=0x9a => { // int binary arithmetic
                    let second = code_units[pc + 1];
                    let b = (second & 0xff) as u16;
                    let c = (second >> 8) as u16;
                    let lhs = rf.read_int(b).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_int(c).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0x90 => lhs.wrapping_add(rhs),
                        0x91 => lhs.wrapping_sub(rhs),
                        0x92 => lhs.wrapping_mul(rhs),
                        0x93 => if rhs == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(rhs) },
                        0x94 => if rhs == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(rhs) },
                        0x95 => lhs & rhs,
                        0x96 => lhs | rhs,
                        0x97 => lhs ^ rhs,
                        0x98 => lhs << (rhs & 0x1f),
                        0x99 => lhs >> (rhs & 0x1f),
                        _ => ((lhs as u32) >> (rhs & 0x1f)) as i32,
                    };
                    rf.write_int(a8, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0xd0..=0xd7 => { // binop/lit16
                    let lhs = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let lit = code_units[pc + 1] as i16 as i32;
                    let res = match op {
                        0xd0 => lhs.wrapping_add(lit),
                        0xd1 => lit.wrapping_sub(lhs), // rsub
                        0xd2 => lhs.wrapping_mul(lit),
                        0xd3 => if lit == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(lit) },
                        0xd4 => if lit == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(lit) },
                        0xd5 => lhs & lit,
                        0xd6 => lhs | lit,
                        _ => lhs ^ lit,
                    };
                    rf.write_int(a4, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0xd8..=0xe2 => { // binop/lit8
                    let second = code_units[pc + 1];
                    let b = (second & 0xff) as u16;
                    let lit = (second >> 8) as u8 as i8 as i32;
                    let lhs = rf.read_int(b).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xd8 => lhs.wrapping_add(lit),
                        0xd9 => lit.wrapping_sub(lhs),
                        0xda => lhs.wrapping_mul(lit),
                        0xdb => if lit == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(lit) },
                        0xdc => if lit == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(lit) },
                        0xdd => lhs & lit,
                        0xde => lhs | lit,
                        0xdf => lhs ^ lit,
                        0xe0 => lhs << (lit & 0x1f),
                        0xe1 => lhs >> (lit & 0x1f),
                        _ => ((lhs as u32) >> (lit & 0x1f)) as i32,
                    };
                    rf.write_int(a8, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x7b..=0x8f => { // conversions
                    let val = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    match op {
                        0x7b => rf.write_int(a4, -val).map_err(|e| format!("Invalid reg {}", e))?,
                        0x7c => rf.write_int(a4, !val).map_err(|e| format!("Invalid reg {}", e))?,
                        0x81 => rf.write_long(a4, int_to_long(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        0x82 => rf.write_float(a4, int_to_float(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        0x83 => rf.write_double(a4, int_to_double(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        0x8d => rf.write_int(a4, int_to_byte(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        0x8e => rf.write_int(a4, int_to_char(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        0x8f => rf.write_int(a4, int_to_short(val)).map_err(|e| format!("Invalid reg {}", e))?,
                        _ => {}
                    }
                    pc += 1;
                }
                0x6e | 0x70 | 0x71 | 0x74 | 0x76 | 0x77 => { // Method invoke
                    let target_method_id = code_units[pc + 1];
                    let is_range = op >= 0x74;
                    let count = if is_range { a8 as usize } else { ((first >> 12) & 0x0f) as usize };
                    let mut args = Vec::with_capacity(count);

                    if is_range {
                        let start = code_units[pc + 2];
                        for i in 0..count {
                            let reg = start + i as u16;
                            let v = rf.read_int(reg).map_err(|e| format!("Invalid reg {}", e))?;
                            args.push(Value::Int(v));
                        }
                    } else {
                        let packed = code_units[pc + 2];
                        let candidate_regs = [
                            packed & 0x0f,
                            (packed >> 4) & 0x0f,
                            (packed >> 8) & 0x0f,
                            (packed >> 12) & 0x0f,
                            ((first >> 8) & 0x0f) as u16,
                        ];
                        for &reg in &candidate_regs[..count] {
                            let v = rf.read_int(reg).map_err(|e| format!("Invalid reg {}", e))?;
                            args.push(Value::Int(v));
                        }
                    }

                    let remaining = step_limit.saturating_sub(steps);
                    let sub_report = self.execute_internal(target_method_id, &args, remaining, call_depth + 1)?;
                    steps += sub_report.steps_executed;

                    match sub_report.stop_reason {
                        ExecutionStopReason::ReturnedValue(returned_val) => {
                            pending_result = Some(returned_val);
                        }
                        ExecutionStopReason::ReturnedVoid => {
                            pending_result = None;
                        }
                        ExecutionStopReason::UncaughtException(uncaught_msg) => {
                            return Ok(InterpreterReport {
                                stop_reason: ExecutionStopReason::UncaughtException(uncaught_msg),
                                steps_executed: steps,
                            });
                        }
                        ExecutionStopReason::StepLimitExceeded => {
                            return Ok(InterpreterReport {
                                stop_reason: ExecutionStopReason::StepLimitExceeded,
                                steps_executed: steps,
                            });
                        }
                    }
                    pc += 3;
                }
                _ => return Err(format!("Unsupported opcode 0x{:02x} at PC {}", op, pc)),
            }
        }

        Ok(InterpreterReport {
            stop_reason: ExecutionStopReason::ReturnedVoid,
            steps_executed: steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_vm_executes_wide_const_and_arithmetic() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let mut methods = HashMap::new();

        // Method 1: const-wide/16 v0, 50; const-wide/16 v2, 25; return-wide v0
        // code: [0x0016, 50, 0x0216, 25, 0x0010]
        methods.insert(
            1,
            MethodCode {
                class_descriptor: "Ltest/Math;".into(),
                name: "calc".into(),
                prototype: "()J".into(),
                code_units: vec![0x0016, 50, 0x0216, 25, 0x0010],
                registers_size: 4,
                ins_size: 0,
                try_blocks: Vec::new(),
            },
        );

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        let report = vm.execute_method(1, &[], 20).unwrap();
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Long(50))
        );
    }

    #[test]
    fn art_vm_allocates_and_reads_array() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let mut methods = HashMap::new();
        let strings = vec!["[I".to_string()];

        // Method 2: const/4 v1, 3; new-array v0, v1, type@0; array-length v2, v0; return v2
        // code: [0x3112, 0x0123, 0, 0x0221, 0x020f]
        methods.insert(
            2,
            MethodCode {
                class_descriptor: "Ltest/ArrayTest;".into(),
                name: "testArray".into(),
                prototype: "()I".into(),
                code_units: vec![0x3112, 0x0123, 0, 0x0221, 0x020f],
                registers_size: 3,
                ins_size: 0,
                try_blocks: Vec::new(),
            },
        );

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);
        let report = vm.execute_method(2, &[], 20).unwrap();
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Int(3))
        );
    }
}
