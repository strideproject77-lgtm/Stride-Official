//! Comprehensive ART-compatible Dalvik virtual machine interpreter.
//!
//! Supports DEX versions 035-041, comprehensive integer, wide, floating point,
//! and object opcodes, arrays, switches, exceptions, field storage, and method invokes.

use std::collections::HashMap;
use crate::types::{
    Value, RegisterFile, int_to_long, int_to_float, int_to_double, long_to_int,
    long_to_float, long_to_double, float_to_int, float_to_long, float_to_double,
    double_to_int, double_to_long, double_to_float, int_to_byte, int_to_char,
    int_to_short, cmp_long, cmpl_float, cmpg_float, cmpl_double, cmpg_double,
};
use crate::heap::ManagedHeap;
use crate::hierarchy::ClassHierarchy;
use crate::exceptions::{TryBlock, ThrownException, RuntimeStackTrace};
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;
use stride_dex_inspector::classes::{ClassInventory, FieldReference, MethodReference};
use crate::diagnostics::{DiagnosticSeverity, RuntimeDiagnostics};

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStopReason {
    ReturnedVoid,
    ReturnedValue(Value),
    StepLimitExceeded,
    UncaughtException(ThrownException),
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
    pub methods: &'a HashMap<u32, MethodCode>,
    pub static_fields: HashMap<(String, String), Value>,
    shared_static_fields: Option<&'a mut HashMap<(String, String), Value>>,
    pub strings: &'a [String],
    pub diagnostics: Option<&'a RuntimeDiagnostics>,
    pub method_references: Option<&'a [MethodReference]>,
    pub method_lookup: Option<&'a HashMap<(String, String, String), u32>>,
    pub type_descriptors: Option<&'a [String]>,
    pub field_references: Option<&'a [FieldReference]>,
    dex_inventories: Option<&'a [ClassInventory]>,
    method_dex_indices: Option<&'a HashMap<u32, usize>>,
}

fn decode_invoke_arguments(
    registers: &RegisterFile,
    register_indices: &[u16],
    prototype: &str,
    is_static: bool,
) -> Result<Vec<Value>, String> {
    let params = prototype.strip_prefix('(')
        .and_then(|tail| tail.split_once(')').map(|(params, _)| params))
        .ok_or_else(|| format!("Malformed method prototype: {}", prototype))?;
    let mut args = Vec::new();
    let mut word = 0usize;
    if !is_static {
        let reg = *register_indices.get(word).ok_or("Missing receiver register")?;
        args.push(Value::Object(registers.read_object(reg).map_err(|r| format!("Invalid receiver register {}", r))?));
        word += 1;
    }
    let bytes = params.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let start = cursor;
        while bytes.get(cursor) == Some(&b'[') { cursor += 1; }
        let kind = *bytes.get(cursor).ok_or("Truncated parameter descriptor")?;
        cursor += 1;
        if kind == b'L' {
            let end = params.get(cursor..).and_then(|s| s.find(';'))
                .ok_or("Unterminated object parameter descriptor")?;
            cursor += end + 1;
        } else if !matches!(kind, b'Z' | b'B' | b'C' | b'S' | b'I' | b'F' | b'J' | b'D') {
            return Err(format!("Unsupported parameter descriptor at byte {}", start));
        }
        let is_array = bytes[start] == b'[';
        let reg = *register_indices.get(word).ok_or("Invoke register count is smaller than prototype")?;
        if is_array || kind == b'L' {
            args.push(Value::Object(registers.read_object(reg).map_err(|r| format!("Invalid object register {}", r))?));
            word += 1;
        } else {
            match kind {
                b'J' => { args.push(Value::Long(registers.read_long(reg).map_err(|r| format!("Invalid wide register {}", r))?)); word += 2; }
                b'D' => { args.push(Value::Double(registers.read_double(reg).map_err(|r| format!("Invalid wide register {}", r))?)); word += 2; }
                b'F' => { args.push(Value::Float(registers.read_float(reg).map_err(|r| format!("Invalid float register {}", r))?)); word += 1; }
                _ => { args.push(Value::Int(registers.read_int(reg).map_err(|r| format!("Invalid integer register {}", r))?)); word += 1; }
            }
        }
    }
    if word != register_indices.len() {
        return Err(format!("DEX invoke register words={} but prototype consumes={}", register_indices.len(), word));
    }
    Ok(args)
}

impl<'a> ArtVm<'a> {
    pub fn new(
        heap: &'a mut ManagedHeap,
        hierarchy: &'a mut ClassHierarchy,
        monitors: &'a mut MonitorTable,
        jni: &'a mut JniNativeRegistry,
        methods: &'a HashMap<u32, MethodCode>,
        strings: &'a [String],
    ) -> Self {
        Self {
            heap,
            hierarchy,
            monitors,
            jni,
            methods,
            static_fields: HashMap::new(),
            shared_static_fields: None,
            strings,
            diagnostics: None,
            method_references: None,
            method_lookup: None,
            type_descriptors: None,
            field_references: None,
            dex_inventories: None,
            method_dex_indices: None,
        }
    }

    /// Shares application static fields with the owning AndroidRuntime so values survive
    /// separate interpreter invocations (for example onCreate -> onStart -> onResume).
    pub fn attach_static_fields(&mut self, fields: &'a mut HashMap<(String, String), Value>) {
        self.shared_static_fields = Some(fields);
    }

    fn read_static_field(&self, key: &(String, String)) -> Value {
        self.shared_static_fields.as_ref()
            .and_then(|fields| fields.get(key).copied())
            .or_else(|| self.static_fields.get(key).copied())
            .unwrap_or(Value::Int(0))
    }

    fn write_static_field(&mut self, key: (String, String), value: Value) {
        if let Some(fields) = self.shared_static_fields.as_mut() {
            (**fields).insert(key, value);
        } else {
            self.static_fields.insert(key, value);
        }
    }

    /// Attaches the session recorder without coupling standalone VM tests to a global logger.
    pub fn attach_diagnostics(&mut self, diagnostics: &'a RuntimeDiagnostics) {
        self.diagnostics = Some(diagnostics);
    }

    /// Connects DEX-local method reference indexes to loaded runtime method IDs.
    /// Attaches DEX-local type_ids and field_ids tables. These indexes are not string_ids.
    pub fn attach_dex_metadata(
        &mut self,
        type_descriptors: &'a [String],
        field_references: &'a [FieldReference],
    ) {
        self.type_descriptors = Some(type_descriptors);
        self.field_references = Some(field_references);
    }

    fn resolve_type_descriptor(&self, index: usize, fallback: &str) -> String {
        self.type_descriptors
            .and_then(|table| table.get(index))
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    }

    fn resolve_field_reference(&self, index: usize) -> Option<&FieldReference> {
        self.field_references.and_then(|table| table.get(index))
    }

    pub fn attach_method_resolver(
        &mut self,
        references: &'a [MethodReference],
        lookup: &'a HashMap<(String, String, String), u32>,
    ) {
        self.method_references = Some(references);
        self.method_lookup = Some(lookup);
    }

    /// Attaches every DEX inventory and the runtime-method-to-DEX ownership map.
    /// DEX string/type/method/field indices are local to the DEX that owns the caller.
    pub fn attach_multi_dex_metadata(
        &mut self,
        inventories: &'a [ClassInventory],
        method_dex_indices: &'a HashMap<u32, usize>,
    ) {
        self.dex_inventories = Some(inventories);
        self.method_dex_indices = Some(method_dex_indices);
    }

    pub fn execute_method(
        &mut self,
        method_id: u32,
        arguments: &[Value],
        step_limit: usize,
    ) -> Result<InterpreterReport, String> {
        let context = self.methods.get(&method_id)
            .map(|method| format!("{}->{}{}", method.class_descriptor, method.name, method.prototype))
            .unwrap_or_else(|| format!("method_id={}", method_id));
        let mut trace = RuntimeStackTrace::new();
        let result = self.execute_internal(method_id, arguments, step_limit, 0, &mut trace);
        if let (Some(diagnostics), Err(error)) = (self.diagnostics, &result) {
            diagnostics.record_detailed(
                DiagnosticSeverity::RuntimeError,
                "ART",
                "INTERPRETER_EXECUTION_FAILED",
                "Dalvik method execution failed",
                Some(&format!("{}: {}", context, error)),
                Some("Use the nested call-site/PC and opcode details to inspect DEX decoding, register use and unresolved framework calls"),
            );
        }
        result
    }

    fn execute_internal(
        &mut self,
        method_id: u32,
        arguments: &[Value],
        step_limit: usize,
        call_depth: usize,
        trace: &mut RuntimeStackTrace,
    ) -> Result<InterpreterReport, String> {
        if call_depth > 128 {
            if let Some(diagnostics) = self.diagnostics {
                diagnostics.record_detailed(
                    DiagnosticSeverity::RuntimeError, "ART", "CALL_DEPTH_LIMIT",
                    "Nested DEX method calls exceeded the runtime depth limit",
                    Some(&format!("method_id={} depth={} limit=128", method_id, call_depth)),
                    Some("Inspect recursive call paths and ensure the application is not stuck in unbounded recursion"),
                );
            }
            return Err("Call depth limit exceeded (128 frames)".into());
        }

        let (code_units, registers_size, ins_size, try_blocks, method_class, method_name, method_proto) = {
            let Some(m) = self.methods.get(&method_id) else {
                if let Some(diagnostics) = self.diagnostics {
                    diagnostics.record_detailed(
                        DiagnosticSeverity::MissingApi, "MethodResolver", "METHOD_ID_NOT_FOUND",
                        "Runtime method ID is not present in the loaded code map",
                        Some(&format!("method_id={} call_depth={}", method_id, call_depth)),
                        Some("Verify DEX method-index to runtime-method-ID mapping and whether the referenced class/method was loaded"),
                    );
                }
                return Err(format!("Method ID {} not found", method_id));
            };
            (
                m.code_units.clone(),
                m.registers_size,
                m.ins_size,
                m.try_blocks.clone(),
                m.class_descriptor.clone(),
                m.name.clone(),
                m.prototype.clone(),
            )
        };

        if code_units.is_empty() {
            return Ok(InterpreterReport {
                stop_reason: ExecutionStopReason::ReturnedVoid,
                steps_executed: 0,
            });
        }

        let mut rf = RegisterFile::new(registers_size);

        // Map arguments into incoming registers (high end of register window)
        let incoming_start = registers_size.saturating_sub(ins_size);
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
            // Restore the active caller's DEX tables after nested calls. A method
            // index such as method@0007 or field@0007 has meaning only in its
            // defining DEX; using classes.dex tables for classes2.dex corrupts dispatch.
            if let (Some(inventories), Some(owners)) = (self.dex_inventories, self.method_dex_indices) {
                if let Some(index) = owners.get(&(method_id as u32)).copied() {
                    if let Some(inventory) = inventories.get(index) {
                        self.strings = &inventory.strings;
                        self.type_descriptors = Some(&inventory.type_descriptors);
                        self.method_references = Some(&inventory.method_references);
                        self.field_references = Some(&inventory.field_references);
                    }
                }
            }
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
                0x1a | 0x1b => { // const-string, const-string/jumbo
                    let str_idx = if op == 0x1a {
                        code_units[pc + 1] as usize
                    } else {
                        (code_units[pc + 1] as usize) | ((code_units[pc + 2] as usize) << 16)
                    };
                    let text = self.strings.get(str_idx).cloned().unwrap_or_default();
                    let str_id = self.heap.allocate_string(&text).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a8, str_id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += if op == 0x1a { 2 } else { 3 };
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
                0x1f => { // check-cast vAA, type@BBBB
                    let obj_id = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    if obj_id != 0 {
                        let type_idx = code_units[pc + 1] as usize;
                        let target_type = self.resolve_type_descriptor(type_idx, "Ljava/lang/Object;");
                        let actual_class = self.heap.get(obj_id)
                            .map_err(|e| format!("{:?}", e))?
                            .class_descriptor()
                            .to_owned();
                        if !self.hierarchy.is_assignable_from(&target_type, &actual_class) {
                            let exception_id = self.heap.allocate_instance("Ljava/lang/ClassCastException;")
                                .map_err(|e| format!("{:?}", e))?;
                            return Ok(InterpreterReport {
                                stop_reason: ExecutionStopReason::UncaughtException(ThrownException {
                                    exception_object: exception_id,
                                    class_descriptor: "Ljava/lang/ClassCastException;".into(),
                                    message: Some(format!("Cannot cast {} to {}", actual_class, target_type)),
                                    stack_trace: trace.clone(),
                                }),
                                steps_executed: steps,
                            });
                        }
                    }
                    pc += 2;
                }
                0x20 => { // instance-of vA, vB, type@CCCC
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let matches = if obj_id == 0 {
                        0
                    } else {
                        let type_idx = code_units[pc + 1] as usize;
                        let target_type = self.resolve_type_descriptor(type_idx, "Ljava/lang/Object;");
                        let actual_obj = self.heap.get(obj_id).map_err(|e| format!("{:?}", e))?;
                        if self.hierarchy.is_assignable_from(&target_type, actual_obj.class_descriptor()) {
                            1
                        } else {
                            0
                        }
                    };
                    rf.write_int(a4, matches).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x21 => { // array-length vA, vB
                    let arr_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let len = self.heap.array_length(arr_id).map_err(|e| format!("{:?}", e))?;
                    rf.write_int(a4, len as i32).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x22 => { // new-instance vAA, type@BBBB
                    let type_idx = code_units[pc + 1] as usize;
                    let descriptor = self.resolve_type_descriptor(type_idx, "Ljava/lang/Object;");
                    let id = self.heap.allocate_instance(&descriptor).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a8, id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x23 => { // new-array vA, vB, type@CCCC
                    let len = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let type_idx = code_units[pc + 1] as usize;
                    let elem_type = self.resolve_type_descriptor(type_idx, "[I");
                    let id = self.heap.allocate_array(&elem_type, len).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a4, id).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x24 => { // filled-new-array {vC, vD, vE, vF, vG}, type@BBBB
                    let type_idx = code_units[pc + 1] as usize;
                    let count = ((first >> 12) & 0x0f) as usize;
                    let packed = code_units[pc + 2];
                    let candidate_regs = [
                        packed & 0x0f,
                        (packed >> 4) & 0x0f,
                        (packed >> 8) & 0x0f,
                        (packed >> 12) & 0x0f,
                        ((first >> 8) & 0x0f) as u16,
                    ];
                    let elem_type = self.resolve_type_descriptor(type_idx, "[I");
                    let arr_id = self.heap.allocate_array(&elem_type, count as i32).map_err(|e| format!("{:?}", e))?;
                    for (i, &reg) in candidate_regs[..count].iter().enumerate() {
                        let v = rf.read_int(reg).map_err(|e| format!("Invalid reg {}", e))?;
                        self.heap.set_array_element(arr_id, i as i32, Value::Int(v)).map_err(|e| format!("{:?}", e))?;
                    }
                    pending_result = Some(Value::Object(arr_id));
                    pc += 3;
                }
                0x25 => { // filled-new-array/range {vCCCC .. vNNNN}, type@BBBB
                    let count = a8 as usize;
                    let type_idx = code_units[pc + 1] as usize;
                    let start_reg = code_units[pc + 2];
                    let elem_type = self.resolve_type_descriptor(type_idx, "[I");
                    let arr_id = self.heap.allocate_array(&elem_type, count as i32).map_err(|e| format!("{:?}", e))?;
                    for i in 0..count {
                        let reg = start_reg + i as u16;
                        let v = rf.read_int(reg).map_err(|e| format!("Invalid reg {}", e))?;
                        self.heap.set_array_element(arr_id, i as i32, Value::Int(v)).map_err(|e| format!("{:?}", e))?;
                    }
                    pending_result = Some(Value::Object(arr_id));
                    pc += 3;
                }
                0x26 => { // fill-array-data vAA, +BBBBBBBB
                    let arr_id = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    let lo = code_units[pc + 1] as u32;
                    let hi = code_units[pc + 2] as u32;
                    let delta = ((hi << 16) | lo) as i32 as isize;
                    let payload_pc = (pc as isize + delta) as usize;
                    if payload_pc + 4 <= code_units.len() && code_units[payload_pc] == 0x0300 {
                        let elem_width = code_units[payload_pc + 1] as usize;
                        let size_lo = code_units[payload_pc + 2] as u32;
                        let size_hi = code_units[payload_pc + 3] as u32;
                        let size = ((size_hi << 16) | size_lo) as usize;
                        let data_start = payload_pc + 4;
                        if elem_width == 1 {
                            for i in 0..size {
                                let word = code_units[data_start + i / 2];
                                let b = if i % 2 == 0 { (word & 0xff) as u8 } else { (word >> 8) as u8 } as i8 as i32;
                                let _ = self.heap.set_array_element(arr_id, i as i32, Value::Int(b));
                            }
                        } else if elem_width == 2 {
                            for i in 0..size {
                                let w = code_units[data_start + i] as i16 as i32;
                                let _ = self.heap.set_array_element(arr_id, i as i32, Value::Int(w));
                            }
                        } else if elem_width == 4 {
                            for i in 0..size {
                                let wlo = code_units[data_start + i * 2] as u32;
                                let whi = code_units[data_start + i * 2 + 1] as u32;
                                let val = ((whi << 16) | wlo) as i32;
                                let _ = self.heap.set_array_element(arr_id, i as i32, Value::Int(val));
                            }
                        }
                    }
                    pc += 3;
                }
                0x27 => { // throw vAA
                    let ex_id = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    if ex_id == 0 {
                        let npe_id = self.heap.allocate_instance("Ljava/lang/NullPointerException;").map_err(|e| format!("{:?}", e))?;
                        return Ok(InterpreterReport {
                            stop_reason: ExecutionStopReason::UncaughtException(ThrownException {
                                exception_object: npe_id,
                                class_descriptor: "Ljava/lang/NullPointerException;".into(),
                                message: Some("throw with null exception".into()),
                                stack_trace: trace.clone(),
                            }),
                            steps_executed: steps,
                        });
                    }
                    let obj = self.heap.get(ex_id).map_err(|e| format!("{:?}", e))?;
                    let ex_desc = obj.class_descriptor().to_owned();

                    // Check if current method has a matching catch block covering PC
                    let mut caught_pc = None;
                    for block in &try_blocks {
                        if block.covers(pc) {
                            if let Some(target) = block.find_handler(&ex_desc, |t, s| self.hierarchy.is_assignable_from(t, s)) {
                                caught_pc = Some(target);
                                break;
                            }
                        }
                    }

                    if let Some(target) = caught_pc {
                        pending_exception = Some(ex_id);
                        pc = target;
                        continue;
                    } else {
                        trace.push_frame(&method_class, &method_name, &method_proto, pc);
                        return Ok(InterpreterReport {
                            stop_reason: ExecutionStopReason::UncaughtException(ThrownException {
                                exception_object: ex_id,
                                class_descriptor: ex_desc,
                                message: None,
                                stack_trace: trace.clone(),
                            }),
                            steps_executed: steps,
                        });
                    }
                }
                0x28 => { // goto +AA
                    let delta = (first >> 8) as u8 as i8 as isize;
                    pc = (pc as isize + delta) as usize;
                }
                0x29 => { // goto/16 +AAAA
                    let delta = code_units[pc + 1] as i16 as isize;
                    pc = (pc as isize + delta) as usize;
                }
                0x2a => { // goto/32 +AAAAAAAA
                    let lo = code_units[pc + 1] as u32;
                    let hi = code_units[pc + 2] as u32;
                    let delta = ((hi << 16) | lo) as i32 as isize;
                    pc = (pc as isize + delta) as usize;
                }
                0x2b => { // packed-switch vAA, +BBBBBBBB
                    let val = rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    let lo = code_units[pc + 1] as u32;
                    let hi = code_units[pc + 2] as u32;
                    let offset = ((hi << 16) | lo) as i32 as isize;
                    let payload_pc = (pc as isize + offset) as usize;
                    if payload_pc + 4 <= code_units.len() && code_units[payload_pc] == 0x0100 {
                        let size = code_units[payload_pc + 1] as usize;
                        let first_key = ((code_units[payload_pc + 3] as u32) << 16 | (code_units[payload_pc + 2] as u32)) as i32;
                        let key_offset = val - first_key;
                        if key_offset >= 0 && (key_offset as usize) < size {
                            let target_idx = payload_pc + 4 + (key_offset as usize) * 2;
                            if target_idx + 1 < code_units.len() {
                                let tlo = code_units[target_idx] as u32;
                                let thi = code_units[target_idx + 1] as u32;
                                let target_delta = ((thi << 16) | tlo) as i32 as isize;
                                pc = (pc as isize + target_delta) as usize;
                                continue;
                            }
                        }
                    }
                    pc += 3;
                }
                0x2c => { // sparse-switch vAA, +BBBBBBBB
                    let val = rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    let lo = code_units[pc + 1] as u32;
                    let hi = code_units[pc + 2] as u32;
                    let offset = ((hi << 16) | lo) as i32 as isize;
                    let payload_pc = (pc as isize + offset) as usize;
                    let mut branched = false;
                    if payload_pc + 2 <= code_units.len() && code_units[payload_pc] == 0x0200 {
                        let size = code_units[payload_pc + 1] as usize;
                        let keys_start = payload_pc + 2;
                        let targets_start = keys_start + size * 2;
                        for i in 0..size {
                            let klo = code_units[keys_start + i * 2] as u32;
                            let khi = code_units[keys_start + i * 2 + 1] as u32;
                            let k = ((khi << 16) | klo) as i32;
                            if k == val {
                                let tlo = code_units[targets_start + i * 2] as u32;
                                let thi = code_units[targets_start + i * 2 + 1] as u32;
                                let target_delta = ((thi << 16) | tlo) as i32 as isize;
                                pc = (pc as isize + target_delta) as usize;
                                branched = true;
                                break;
                            }
                        }
                    }
                    if !branched {
                        pc += 3;
                    }
                }
                0x2d..=0x31 => { // comparisons: cmpl-float, cmpg-float, cmpl-double, cmpg-double, cmp-long
                    match op {
                        0x2d => { // cmpl-float
                            let second = code_units[pc + 1];
                            let b = rf.read_float((second & 0xff) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            let c = rf.read_float((second >> 8) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a8, cmpl_float(b, c)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x2e => { // cmpg-float
                            let second = code_units[pc + 1];
                            let b = rf.read_float((second & 0xff) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            let c = rf.read_float((second >> 8) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a8, cmpg_float(b, c)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x2f => { // cmpl-double
                            let second = code_units[pc + 1];
                            let b = rf.read_double((second & 0xff) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            let c = rf.read_double((second >> 8) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a8, cmpl_double(b, c)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x30 => { // cmpg-double
                            let second = code_units[pc + 1];
                            let b = rf.read_double((second & 0xff) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            let c = rf.read_double((second >> 8) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a8, cmpg_double(b, c)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x31 => { // cmp-long
                            let second = code_units[pc + 1];
                            let b = rf.read_long((second & 0xff) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            let c = rf.read_long((second >> 8) as u16).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a8, cmp_long(b, c)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        _ => {}
                    }
                    pc += 2;
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
                0x45 => { // aget-wide
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = self.heap.get_array_element(arr_id, idx).map_err(|e| format!("{:?}", e))?;
                    rf.write_long(a8, val.as_long()).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x46 => { // aget-object
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = self.heap.get_array_element(arr_id, idx).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a8, val.as_object().unwrap_or(0)).map_err(|e| format!("Invalid reg {}", e))?;
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
                0x4c => { // aput-wide
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = rf.read_long(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_array_element(arr_id, idx, Value::Long(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x4d => { // aput-object
                    let second = code_units[pc + 1];
                    let arr_reg = (second & 0xff) as u16;
                    let idx_reg = (second >> 8) as u16;
                    let arr_id = rf.read_object(arr_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let idx = rf.read_int(idx_reg).map_err(|e| format!("Invalid reg {}", e))?;
                    let val = rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_array_element(arr_id, idx, Value::Object(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x52 | 0x55..=0x58 => { // iget, iget-boolean/byte/char/short
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = self.heap.get_instance_field(obj_id, &field_name).map_err(|e| format!("{:?}", e))?;
                    rf.write_int(a4, val.as_int()).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x53 => { // iget-wide
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = self.heap.get_instance_field(obj_id, &field_name).map_err(|e| format!("{:?}", e))?;
                    rf.write_long(a4, val.as_long()).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x54 => { // iget-object
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = self.heap.get_instance_field(obj_id, &field_name).map_err(|e| format!("{:?}", e))?;
                    rf.write_object(a4, val.as_object().unwrap_or(0)).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0x59 | 0x5c..=0x5f => { // iput, iput-boolean/byte/char/short
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = rf.read_int(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_instance_field(obj_id, &field_name, Value::Int(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x5a => { // iput-wide
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = rf.read_long(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_instance_field(obj_id, &field_name, Value::Long(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x5b => { // iput-object
                    let obj_id = rf.read_object(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let field_idx = code_units[pc + 1] as usize;
                    let field_name = self.resolve_field_reference(field_idx).map(|field| field.name.clone()).unwrap_or_else(|| format!("field_{}", field_idx));
                    let val = rf.read_object(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    self.heap.set_instance_field(obj_id, &field_name, Value::Object(val)).map_err(|e| format!("{:?}", e))?;
                    pc += 2;
                }
                0x60..=0x66 => { // sget variants
                    let field_idx = code_units[pc + 1] as usize;
                    let field = self.resolve_field_reference(field_idx);
                    let field_name = field.map(|reference| reference.name.clone()).unwrap_or_else(|| format!("sfield_{}", field_idx));
                    let field_owner = field.map(|reference| reference.class_descriptor.clone()).unwrap_or_else(|| method_class.clone());
                    let key = (field_owner, field_name);
                    let val = self.read_static_field(&key);
                    if op == 0x61 {
                        rf.write_long(a8, val.as_long()).map_err(|e| format!("Invalid reg {}", e))?;
                    } else if op == 0x62 {
                        rf.write_object(a8, val.as_object().unwrap_or(0)).map_err(|e| format!("Invalid reg {}", e))?;
                    } else {
                        rf.write_int(a8, val.as_int()).map_err(|e| format!("Invalid reg {}", e))?;
                    }
                    pc += 2;
                }
                0x67..=0x6d => { // sput variants
                    let field_idx = code_units[pc + 1] as usize;
                    let field = self.resolve_field_reference(field_idx);
                    let field_name = field.map(|reference| reference.name.clone()).unwrap_or_else(|| format!("sfield_{}", field_idx));
                    let field_owner = field.map(|reference| reference.class_descriptor.clone()).unwrap_or_else(|| method_class.clone());
                    let key = (field_owner, field_name);
                    let val = if op == 0x68 {
                        Value::Long(rf.read_long(a8).map_err(|e| format!("Invalid reg {}", e))?)
                    } else if op == 0x69 {
                        Value::Object(rf.read_object(a8).map_err(|e| format!("Invalid reg {}", e))?)
                    } else {
                        Value::Int(rf.read_int(a8).map_err(|e| format!("Invalid reg {}", e))?)
                    };
                    self.write_static_field(key, val);
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
                0x9b..=0xa5 => { // long binary arithmetic
                    let second = code_units[pc + 1];
                    let b = (second & 0xff) as u16;
                    let c = (second >> 8) as u16;
                    let lhs = rf.read_long(b).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_long(c).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0x9b => lhs.wrapping_add(rhs),
                        0x9c => lhs.wrapping_sub(rhs),
                        0x9d => lhs.wrapping_mul(rhs),
                        0x9e => if rhs == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(rhs) },
                        0x9f => if rhs == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(rhs) },
                        0xa0 => lhs & rhs,
                        0xa1 => lhs | rhs,
                        0xa2 => lhs ^ rhs,
                        0xa3 => lhs << (rhs & 0x3f),
                        0xa4 => lhs >> (rhs & 0x3f),
                        _ => ((lhs as u64) >> (rhs & 0x3f)) as i64,
                    };
                    rf.write_long(a8, res).map_err(|e| format!("Invalid reg {}", e))?;
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
                0x7b..=0x8f => { // conversions and unary operations
                    match op {
                        0x7b => { // neg-int
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, -v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x7c => { // not-int
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, !v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x7d => { // neg-long
                            let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_long(a4, -v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x7e => { // not-long
                            let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_long(a4, !v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x7f => { // neg-float
                            let v = rf.read_float(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_float(a4, -v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x80 => { // neg-double
                            let v = rf.read_double(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_double(a4, -v).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x81 => { // int-to-long
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_long(a4, int_to_long(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x82 => { // int-to-float
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_float(a4, int_to_float(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x83 => { // int-to-double
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_double(a4, int_to_double(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x84 => { // long-to-int
                            let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, long_to_int(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x85 => { // long-to-float
                            let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_float(a4, long_to_float(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x86 => { // long-to-double
                            let v = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_double(a4, long_to_double(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x87 => { // float-to-int
                            let v = rf.read_float(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, float_to_int(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x88 => { // float-to-long
                            let v = rf.read_float(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_long(a4, float_to_long(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x89 => { // float-to-double
                            let v = rf.read_float(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_double(a4, float_to_double(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8a => { // double-to-int
                            let v = rf.read_double(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, double_to_int(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8b => { // double-to-long
                            let v = rf.read_double(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_long(a4, double_to_long(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8c => { // double-to-float
                            let v = rf.read_double(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_float(a4, double_to_float(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8d => { // int-to-byte
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, int_to_byte(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8e => { // int-to-char
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, int_to_char(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        0x8f => { // int-to-short
                            let v = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                            rf.write_int(a4, int_to_short(v)).map_err(|e| format!("Invalid reg {}", e))?;
                        }
                        _ => {}
                    }
                    pc += 1;
                }
                0xa6..=0xaa => { // float 3-register binary arithmetic
                    let second = code_units[pc + 1];
                    let b = (second & 0xff) as u16;
                    let c = (second >> 8) as u16;
                    let lhs = rf.read_float(b).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_float(c).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xa6 => lhs + rhs,
                        0xa7 => lhs - rhs,
                        0xa8 => lhs * rhs,
                        0xa9 => lhs / rhs,
                        _ => lhs % rhs,
                    };
                    rf.write_float(a8, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0xab..=0xaf => { // double 3-register binary arithmetic
                    let second = code_units[pc + 1];
                    let b = (second & 0xff) as u16;
                    let c = (second >> 8) as u16;
                    let lhs = rf.read_double(b).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_double(c).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xab => lhs + rhs,
                        0xac => lhs - rhs,
                        0xad => lhs * rhs,
                        0xae => lhs / rhs,
                        _ => lhs % rhs,
                    };
                    rf.write_double(a8, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 2;
                }
                0xb0..=0xba => { // int /2addr
                    let lhs = rf.read_int(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_int(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xb0 => lhs.wrapping_add(rhs),
                        0xb1 => lhs.wrapping_sub(rhs),
                        0xb2 => lhs.wrapping_mul(rhs),
                        0xb3 => if rhs == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(rhs) },
                        0xb4 => if rhs == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(rhs) },
                        0xb5 => lhs & rhs,
                        0xb6 => lhs | rhs,
                        0xb7 => lhs ^ rhs,
                        0xb8 => lhs << (rhs & 0x1f),
                        0xb9 => lhs >> (rhs & 0x1f),
                        _ => ((lhs as u32) >> (rhs & 0x1f)) as i32,
                    };
                    rf.write_int(a4, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0xbb..=0xc5 => { // long /2addr
                    let lhs = rf.read_long(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_long(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xbb => lhs.wrapping_add(rhs),
                        0xbc => lhs.wrapping_sub(rhs),
                        0xbd => lhs.wrapping_mul(rhs),
                        0xbe => if rhs == 0 { return Err("Division by zero".into()); } else { lhs.wrapping_div(rhs) },
                        0xbf => if rhs == 0 { return Err("Remainder by zero".into()); } else { lhs.wrapping_rem(rhs) },
                        0xc0 => lhs & rhs,
                        0xc1 => lhs | rhs,
                        0xc2 => lhs ^ rhs,
                        0xc3 => lhs << (rhs & 0x3f),
                        0xc4 => lhs >> (rhs & 0x3f),
                        _ => ((lhs as u64) >> (rhs & 0x3f)) as i64,
                    };
                    rf.write_long(a4, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0xc6..=0xca => { // float /2addr
                    let lhs = rf.read_float(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_float(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xc6 => lhs + rhs,
                        0xc7 => lhs - rhs,
                        0xc8 => lhs * rhs,
                        0xc9 => lhs / rhs,
                        _ => lhs % rhs,
                    };
                    rf.write_float(a4, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0xcb..=0xcf => { // double /2addr
                    let lhs = rf.read_double(a4).map_err(|e| format!("Invalid reg {}", e))?;
                    let rhs = rf.read_double(b4).map_err(|e| format!("Invalid reg {}", e))?;
                    let res = match op {
                        0xcb => lhs + rhs,
                        0xcc => lhs - rhs,
                        0xcd => lhs * rhs,
                        0xce => lhs / rhs,
                        _ => lhs % rhs,
                    };
                    rf.write_double(a4, res).map_err(|e| format!("Invalid reg {}", e))?;
                    pc += 1;
                }
                0x6e | 0x6f | 0x70 | 0x71 | 0x72 | 0x74 | 0x75 | 0x76 | 0x77 | 0x78 => { // Method invoke family
                    let dex_method_index = code_units[pc + 1];
                    let is_range = op >= 0x74;
                    let count = if is_range { a8 as usize } else { ((first >> 12) & 0x0f) as usize };
                    let mut argument_registers = Vec::with_capacity(count);

                    if is_range {
                        let start = code_units[pc + 2];
                        for i in 0..count {
                            argument_registers.push(start.checked_add(i as u16)
                                .ok_or_else(|| "Invoke register range overflow".to_string())?);
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
                        argument_registers.extend_from_slice(&candidate_regs[..count]);
                    }

                    // DEX invoke operands index the DEX method_ids table, not the
                    // runtime's compact method-code map. Resolve the full signature first.
                    let reference = self.method_references
                        .and_then(|references| references.get(dex_method_index as usize));
                    let signature = reference.map(|r| (
                        r.class_descriptor.as_str(), r.name.as_str(), r.prototype.as_str()
                    ));
                    let runtime_method_id = if let Some((class, name, proto)) = signature {
                        self.method_lookup
                            .and_then(|lookup| lookup.get(&(class.to_owned(), name.to_owned(), proto.to_owned())).copied())
                            
                    } else if self.method_references.is_none() {
                        Some(u32::from(dex_method_index))
                    } else {
                        None
                    };
                    let exact_target_method = runtime_method_id.and_then(|id| self.methods.get(&id));
                    let (target_class, target_name, target_proto) = if let Some(r) = reference {
                        (r.class_descriptor.as_str(), r.name.as_str(), r.prototype.as_str())
                    } else if let Some(target) = exact_target_method {
                        (target.class_descriptor.as_str(), target.name.as_str(), target.prototype.as_str())
                    } else {
                        ("<unknown>", "<unknown>", "()V")
                    };

                    let is_static_invoke = matches!(op, 0x71 | 0x77);
                    let args = decode_invoke_arguments(&rf, &argument_registers, target_proto, is_static_invoke)
                        .map_err(|error| format!(
                            "Invalid invoke arguments for {}->{}{} at PC {}: {}",
                            target_class, target_name, target_proto, pc, error
                        ))?;

                    // invoke-virtual and invoke-interface must dispatch against the
                    // receiver's runtime class, not merely the declaring class in method_ids.
                    // Without this, overridden callbacks and inherited app methods silently
                    // execute the base implementation (or fail when the interface is abstract).
                    let mut dispatched_method_id = runtime_method_id;
                    if matches!(op, 0x6e | 0x72 | 0x74 | 0x78) {
                        if let Some(receiver_id) = args.first().and_then(Value::as_object).filter(|id| *id != 0) {
                            if let Ok(receiver_object) = self.heap.get(receiver_id) {
                                let receiver_class = receiver_object.class_descriptor().to_owned();
                                if let Some((_, implementation)) = self.hierarchy.resolve_virtual_method(
                                    &receiver_class, target_name, target_proto,
                                ) {
                                    dispatched_method_id = Some(implementation.method_index);
                                }
                            }
                        }
                    }
                    let target_method = dispatched_method_id.and_then(|id| self.methods.get(&id));

                    // Framework and native references are dispatched by their complete
                    // DEX signature even when no app-defined MethodCode exists.
                    let mut native_handler = self.jni.find(target_class, target_name, target_proto).cloned();
                    // DEX method_ids can name an app subclass even when the method is
                    // implemented by a framework ancestor. First use linked inheritance.
                    if native_handler.is_none() {
                        let mut owner = target_class.to_owned();
                        let mut visited = std::collections::HashSet::new();
                        while visited.insert(owner.clone()) {
                            let Some(class) = self.hierarchy.get_class(&owner) else { break; };
                            let Some(superclass) = class.superclass_descriptor.as_deref() else { break; };
                            native_handler = self.jni.find(superclass, target_name, target_proto).cloned();
                            if native_handler.is_some() { break; }
                            owner = superclass.to_owned();
                        }
                    }
                    // Some framework ancestors are not materialized in the app class
                    // hierarchy. A unique framework bridge with the exact DEX name and
                    // prototype is still safe to resolve (ambiguous signatures are rejected).
                    if native_handler.is_none() {
                        native_handler = self.jni.find_unique_framework_signature(target_name, target_proto).cloned();
                    }
                    if target_method.is_none() {
                        if let Some(handler) = native_handler {
                            let mut env = crate::jni::JniEnv::new(self.heap);
                            pending_result = handler(&mut env, &args).map_err(|e| format!("{:?}", e))?;
                            pc += 3;
                            continue;
                        }
                        if let Some(diagnostics) = self.diagnostics {
                            diagnostics.record_detailed(
                                DiagnosticSeverity::MissingApi, "MethodResolver", "INVOKE_TARGET_UNRESOLVED",
                                "DEX method reference could not be resolved to app code or a registered framework/JNI bridge",
                                Some(&format!(
                                    "dex_method_index={} target={}->{}{} caller={}->{}{} pc={}",
                                    dex_method_index, target_class, target_name, target_proto,
                                    method_class, method_name, method_proto, pc
                                )),
                                Some("Resolve the DEX method_ids entry to a loaded class method or implement the exact framework/JNI signature"),
                            );
                        }
                        return Err(format!(
                            "Unresolved DEX method reference #{}: {}->{}{}",
                            dex_method_index, target_class, target_name, target_proto
                        ));
                    }

                    let runtime_method_id = dispatched_method_id.expect("target method implies runtime ID");
                    let remaining = step_limit.saturating_sub(steps);
                    let sub_report = self.execute_internal(runtime_method_id, &args, remaining, call_depth + 1, trace)
                        .map_err(|error| format!(
                            "Call site {}->{}{} at PC {}: {}",
                            method_class, method_name, method_proto, pc, error
                        ))?;
                    steps += sub_report.steps_executed;

                    match sub_report.stop_reason {
                        ExecutionStopReason::ReturnedValue(returned_val) => {
                            pending_result = Some(returned_val);
                        }
                        ExecutionStopReason::ReturnedVoid => {
                            pending_result = None;
                        }
                        ExecutionStopReason::UncaughtException(mut uncaught_ex) => {
                            trace.push_frame(&method_class, &method_name, &method_proto, pc);
                            let mut caught_pc = None;
                            for block in &try_blocks {
                                if block.covers(pc) {
                                    if let Some(target) = block.find_handler(&uncaught_ex.class_descriptor, |t, s| self.hierarchy.is_assignable_from(t, s)) {
                                        caught_pc = Some(target);
                                        break;
                                    }
                                }
                            }
                            if let Some(target) = caught_pc {
                                pending_exception = Some(uncaught_ex.exception_object);
                                pc = target;
                                continue;
                            } else {
                                uncaught_ex.stack_trace = trace.clone();
                                return Ok(InterpreterReport {
                                    stop_reason: ExecutionStopReason::UncaughtException(uncaught_ex),
                                    steps_executed: steps,
                                });
                            }
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
                _ => {
                    if let Some(diagnostics) = self.diagnostics {
                        diagnostics.record_detailed(
                            DiagnosticSeverity::RuntimeError, "ART", "UNSUPPORTED_DEX_OPCODE",
                            "Interpreter encountered an unsupported DEX opcode",
                            Some(&format!(
                                "opcode=0x{:02x} pc={} method={}->{}{}",
                                op, pc, method_class, method_name, method_proto
                            )),
                            Some("Check the DEX version and opcode format; add a verified implementation before retrying execution"),
                        );
                    }
                    return Err(format!("Unsupported opcode 0x{:02x} at PC {}", op, pc));
                },
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
    #[test]
    fn each_method_uses_its_own_dex_string_table() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let methods = HashMap::from([
            (1u32, MethodCode {
                class_descriptor: "Ltest/Primary;".into(),
                name: "read".into(),
                prototype: "()Ljava/lang/String;".into(),
                code_units: vec![0x001a, 0x0000, 0x0011],
                registers_size: 1, ins_size: 0, try_blocks: vec![],
            }),
            (2u32, MethodCode {
                class_descriptor: "Ltest/Secondary;".into(),
                name: "read".into(),
                prototype: "()Ljava/lang/String;".into(),
                code_units: vec![0x001a, 0x0000, 0x0011],
                registers_size: 1, ins_size: 0, try_blocks: vec![],
            }),
        ]);
        let inventories = vec![
            ClassInventory {
                classes: vec![], method_references: vec![], field_references: vec![],
                strings: vec!["primary-dex".into()], type_descriptors: vec![],
            },
            ClassInventory {
                classes: vec![], method_references: vec![], field_references: vec![],
                strings: vec!["secondary-dex".into()], type_descriptors: vec![],
            },
        ];
        let method_owners = HashMap::from([(1u32, 0usize), (2u32, 1usize)]);
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        vm.attach_multi_dex_metadata(&inventories, &method_owners);

        for (method_id, expected) in [(1u32, "primary-dex"), (2u32, "secondary-dex")] {
            let report = vm.execute_method(method_id, &[], 16).unwrap();
            let ExecutionStopReason::ReturnedValue(Value::Object(string_id)) = report.stop_reason else {
                panic!("const-string method must return a String object");
            };
            assert_eq!(vm.heap.get_string(string_id).unwrap(), expected);
        }
    }

    #[test]
    fn application_static_fields_survive_interpreter_recreation() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let strings = vec!["counter".to_string()];
        let methods = HashMap::from([
            (1u32, MethodCode {
                class_descriptor: "LTest;".into(), name: "writeCounter".into(), prototype: "()V".into(),
                code_units: vec![0x7012, 0x0067, 0x0000, 0x000e],
                registers_size: 1, ins_size: 0, try_blocks: vec![],
            }),
            (2u32, MethodCode {
                class_descriptor: "LTest;".into(), name: "readCounter".into(), prototype: "()I".into(),
                code_units: vec![0x0060, 0x0000, 0x000f],
                registers_size: 1, ins_size: 0, try_blocks: vec![],
            }),
        ]);
        let mut app_static_fields = HashMap::new();

        {
            let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);
            vm.attach_static_fields(&mut app_static_fields);
            let report = vm.execute_method(1, &[], 32).expect("writer method should execute");
            assert_eq!(report.steps_executed, 3);
        }

        {
            let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);
            vm.attach_static_fields(&mut app_static_fields);
            let report = vm.execute_method(2, &[], 32).expect("reader method should execute");
            assert_eq!(report.stop_reason, ExecutionStopReason::ReturnedValue(Value::Int(7)));
        }
    }


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
        // code: [0x3112, 0x1023, 0, 0x0221, 0x020f]
        methods.insert(
            2,
            MethodCode {
                class_descriptor: "Ltest/ArrayTest;".into(),
                name: "testArray".into(),
                prototype: "()I".into(),
                code_units: vec![0x3112, 0x1023, 0, 0x0221, 0x020f],
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

    #[test]
    fn art_vm_executes_packed_switch() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let mut methods = HashMap::new();

        // PC 0: const/4 v0, 2
        // PC 1: packed-switch v0, +4 -> payload at PC 5
        // PC 4: default return
        // PC 5..14: packed-switch payload (three keys: 1, 2, 3)
        // Case targets are executable instructions after the payload.
        let code = vec![
            0x2012, // PC 0: const/4 v0, 2
            0x002b, 0x0004, 0x0000, // PC 1: packed-switch v0, +4
            0x000f, // PC 4: return v0 (default)
            0x0100, // PC 5: packed-switch payload ident
            0x0003, // PC 6: size = 3
            0x0001, 0x0000, // PC 7..8: first_key = 1
            0x000e, 0x0000, // PC 9..10: key 1 -> PC 15
            0x0011, 0x0000, // PC 11..12: key 2 -> PC 18
            0x0014, 0x0000, // PC 13..14: key 3 -> PC 21
            0x0013, 111, // PC 15..16: const/16 v0, 111
            0x000f, // PC 17: return v0
            0x0013, 999, // PC 18..19: const/16 v0, 999
            0x000f, // PC 20: return v0
            0x0013, 333, // PC 21..22: const/16 v0, 333
            0x000f, // PC 23: return v0
        ];

        methods.insert(
            3,
            MethodCode {
                class_descriptor: "Ltest/Switch;".into(),
                name: "testSwitch".into(),
                prototype: "()I".into(),
                code_units: code,
                registers_size: 2,
                ins_size: 0,
                try_blocks: Vec::new(),
            },
        );

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        let report = vm.execute_method(3, &[], 20).unwrap();
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Int(999))
        );
    }

    #[test]
    fn art_vm_executes_2addr_and_conversions() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let mut methods = HashMap::new();

        // v0 = 10, v1 = 5
        // add-int/2addr v0, v1 -> v0 = 15 (0x10b0)
        // int-to-long v2, v0 (0x0281)
        // return-wide v2 (0x0210)
        let code = vec![
            0x0013, 10, // const/16 v0, 10
            0x0113, 5,  // const/16 v1, 5
            0x10b0,     // add-int/2addr v0, v1
            0x0281,     // int-to-long v2, v0
            0x0210,     // return-wide v2
        ];

        methods.insert(
            4,
            MethodCode {
                class_descriptor: "Ltest/Math2Addr;".into(),
                name: "test2Addr".into(),
                prototype: "()J".into(),
                code_units: code,
                registers_size: 4,
                ins_size: 0,
                try_blocks: Vec::new(),
            },
        );

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        let report = vm.execute_method(4, &[], 20).unwrap();
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Long(15))
        );
    }

    #[test]
    fn art_vm_executes_filled_new_array() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let mut methods = HashMap::new();
        let strings = vec!["[I".to_string()];

        // v0 = 42, v1 = 99
        // filled-new-array {v0, v1}, type@0 (0x2024, 0x0000, 0x0010)
        // move-result-object v2 (0x020c)
        // aget v3, v2, v1_idx(0) (0x0344, 0x0002) -> with v0 as index
        let code = vec![
            0x0013, 42, // const/16 v0, 42
            0x0113, 99, // const/16 v1, 99
            0x2024, 0x0000, 0x0010, // filled-new-array {v0, v1}, type@0
            0x020c, // move-result-object v2
            0x0012, // const/4 v0, 0 (index 0)
            0x0344, 0x0002, // aget v3, v2, v0
            0x030f, // return v3
        ];

        methods.insert(
            5,
            MethodCode {
                class_descriptor: "Ltest/FilledArray;".into(),
                name: "testFilledArray".into(),
                prototype: "()I".into(),
                code_units: code,
                registers_size: 4,
                ins_size: 0,
                try_blocks: Vec::new(),
            },
        );

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &strings);
        let report = vm.execute_method(5, &[], 20).unwrap();
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Int(42))
        );
    }
    #[test]
    fn invoke_virtual_dispatches_to_the_receivers_most_specific_override() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();

        let mut base = crate::hierarchy::RuntimeClass::new(
            "Ltest/Base;", Some("Ljava/lang/Object;"),
        );
        base.register_method("value", "()I", 2, false);
        hierarchy.register_class(base);

        let mut child = crate::hierarchy::RuntimeClass::new(
            "Ltest/Child;", Some("Ltest/Base;"),
        );
        child.register_method("value", "()I", 3, false);
        hierarchy.register_class(child);

        let methods = HashMap::from([
            (1u32, MethodCode {
                class_descriptor: "Ltest/Caller;".into(),
                name: "run".into(),
                prototype: "()I".into(),
                // new Child; invoke-virtual Base.value(); move-result; return
                code_units: vec![0x0022, 0x0000, 0x106e, 0x0000, 0x0000, 0x010a, 0x010f],
                registers_size: 2,
                ins_size: 0,
                try_blocks: vec![],
            }),
            (2u32, MethodCode {
                class_descriptor: "Ltest/Base;".into(),
                name: "value".into(),
                prototype: "()I".into(),
                code_units: vec![0x1012, 0x000f], // return 1
                registers_size: 1,
                ins_size: 0,
                try_blocks: vec![],
            }),
            (3u32, MethodCode {
                class_descriptor: "Ltest/Child;".into(),
                name: "value".into(),
                prototype: "()I".into(),
                code_units: vec![0x2012, 0x000f], // return 2
                registers_size: 1,
                ins_size: 0,
                try_blocks: vec![],
            }),
        ]);
        let types = vec!["Ltest/Child;".to_string()];
        let fields = Vec::new();
        let references = vec![MethodReference {
            method_index: 0,
            class_descriptor: "Ltest/Base;".into(),
            name: "value".into(),
            prototype: "()I".into(),
        }];
        let lookup = HashMap::from([
            (("Ltest/Base;".into(), "value".into(), "()I".into()), 2u32),
            (("Ltest/Child;".into(), "value".into(), "()I".into()), 3u32),
        ]);

        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        vm.attach_dex_metadata(&types, &fields);
        vm.attach_method_resolver(&references, &lookup);
        let report = vm.execute_method(1, &[], 32).expect("caller method should execute");
        assert_eq!(
            report.stop_reason,
            ExecutionStopReason::ReturnedValue(Value::Int(2)),
            "invoke-virtual must call Child.value(), not Base.value()",
        );
    }

    #[test]
    fn records_missing_method_ids_in_attached_session_diagnostics() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let methods = HashMap::new();
        let diagnostics = RuntimeDiagnostics::new();
        let mut vm = ArtVm::new(
            &mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[],
        );
        vm.attach_diagnostics(&diagnostics);

        assert!(vm.execute_method(77, &[], 10).is_err());
        let report = diagnostics.export_text();
        assert!(report.contains("METHOD_ID_NOT_FOUND"), "{report}");
        assert!(report.contains("INTERPRETER_EXECUTION_FAILED"), "{report}");
    }

    #[test]
    fn resolves_new_instance_type_index_through_type_ids() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let methods = HashMap::from([(
            1u32,
            MethodCode {
                class_descriptor: "Ltest/Factory;".into(),
                name: "create".into(),
                prototype: "()Ljava/lang/Object;".into(),
                code_units: vec![0x0022, 0x0000, 0x0011],
                registers_size: 1,
                ins_size: 0,
                try_blocks: vec![],
            },
        )]);
        let types = vec!["Ltest/Allocated;".to_string()];
        let fields = Vec::new();
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        vm.attach_dex_metadata(&types, &fields);
        let report = vm.execute_method(1, &[], 16).expect("new-instance should execute");
        let object_id = match report.stop_reason {
            ExecutionStopReason::ReturnedValue(Value::Object(object_id)) => object_id,
            other => panic!("new-instance returned an unexpected result: {:?}", other),
        };
        assert_eq!(heap.get(object_id).unwrap().class_descriptor(), "Ltest/Allocated;");
    }

    #[test]
    fn static_field_operands_resolve_owner_and_name_from_field_ids() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let methods = HashMap::from([(
            1u32,
            MethodCode {
                class_descriptor: "Ltest/Caller;".into(),
                name: "write".into(),
                prototype: "()V".into(),
                code_units: vec![0x7012, 0x0067, 0x0000, 0x000e],
                registers_size: 1,
                ins_size: 0,
                try_blocks: vec![],
            },
        )]);
        let types = Vec::new();
        let fields = vec![FieldReference {
            field_index: 0,
            class_descriptor: "Ltest/Owner;".into(),
            name: "score".into(),
            field_type: "I".into(),
        }];
        let mut static_fields = HashMap::new();
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        vm.attach_dex_metadata(&types, &fields);
        vm.attach_static_fields(&mut static_fields);
        let report = vm.execute_method(1, &[], 16).expect("sput should execute");
        assert_eq!(report.stop_reason, ExecutionStopReason::ReturnedVoid);
        drop(vm);
        assert_eq!(static_fields.get(&("Ltest/Owner;".into(), "score".into())), Some(&Value::Int(7)));
    }

    #[test]
    fn art_vm_executes_double_comparison_opcodes() {
        let mut heap = ManagedHeap::default();
        let mut hierarchy = ClassHierarchy::new();
        let mut monitors = MonitorTable::new();
        let mut jni = JniNativeRegistry::new();
        let methods = HashMap::from([(
            1u32,
            MethodCode {
                class_descriptor: "Ltest/DoubleCompare;".into(),
                name: "compare".into(),
                prototype: "()I".into(),
                code_units: vec![
                    0x0018, 0x0000, 0x0000, 0x0000, 0x3ff0, // v0 = 1.0
                    0x0218, 0x0000, 0x0000, 0x0000, 0x4000, // v2 = 2.0
                    0x042f, 0x0200, // cmpl-double v4, v0, v2
                    0x040f, // return v4
                ],
                registers_size: 5,
                ins_size: 0,
                try_blocks: vec![],
            },
        )]);
        let mut vm = ArtVm::new(&mut heap, &mut hierarchy, &mut monitors, &mut jni, &methods, &[]);
        let report = vm.execute_method(1, &[], 32).expect("cmpl-double should execute");
        assert_eq!(report.stop_reason, ExecutionStopReason::ReturnedValue(Value::Int(-1)));
    }

}
