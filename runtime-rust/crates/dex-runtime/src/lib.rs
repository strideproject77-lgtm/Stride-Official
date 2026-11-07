//! A deliberately small, bounds-checked Dalvik integer interpreter.
//!
//! This is an execution primitive, not ART. It supports a limited integer opcode
//! subset and a metadata-only class/method registry. It does not provide Android
//! framework classes, inheritance dispatch, JNI, or native library execution.

pub mod types;
pub mod heap;
pub mod hierarchy;
pub mod sync;
pub mod exceptions;
pub mod jni;
pub mod framework;
pub mod interpreter;
pub mod loader;
pub mod graphics;
pub mod launcher;

pub use heap::{ManagedHeap, HeapObject, HeapError, InstanceObject, ArrayObject, StringObject, GcReport};
pub use types::{Value, RegisterFile};
pub use hierarchy::{ClassHierarchy, RuntimeClass};
pub use sync::{MonitorTable, SyncError};
pub use exceptions::{TryBlock, CatchHandler, ThrownException, RuntimeStackTrace, StackFrameInfo};
pub use jni::{JniNativeRegistry, JniEnv, NativeMethodKey};
pub use framework::{
    AndroidEnvironment, SharedPreferences, ActivityLifecycleState, Bundle, Intent,
    PackageInfo, ActivityInfo, ServiceInfo, AssetManager, Resources,
};
pub use interpreter::{ArtVm, MethodCode, InterpreterReport, ExecutionStopReason};
pub use loader::{AppClassLoader, LoadedClass, LoadedMethod, ClassInitState};
pub use graphics::{
    Surface, DisplayMetrics, MotionEvent, PointerCoords, KeyEvent, InputQueue, AudioTrack, AudioTrackState,
    ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_CANCEL, KEYCODE_BACK,
};
pub use launcher::{AndroidRuntime, LaunchReport, LaunchStatus};

use std::collections::{HashMap, HashSet};
use stride_dex_inspector::classes::{
    ClassDefinition, ClassInventory, FieldDefinition, FieldReference, MethodDefinition, MethodReference,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    DuplicateClass(String),
    InvalidDexIndex,
    InvalidMethodIndex,
    InvalidFieldIndex,
    ClassNotFound(String),
    MethodNotFound { class: String, name: String, prototype: String },
    FieldNotFound { class: String, name: String, field_type: String },
    UnsupportedFieldType(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMethod {
    pub class: ClassDefinition,
    pub method: MethodDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedField {
    pub class: ClassDefinition,
    pub field: FieldDefinition,
}

impl ResolvedField {
    /// True only for primitive descriptors represented by the i32 field store.
    pub fn supports_integer_storage(&self) -> bool {
        matches!(self.field.field_type.as_str(), "I" | "Z" | "B" | "C" | "S")
    }
}

#[derive(Debug, Clone)]
struct DexSymbolTables {
    methods: Vec<MethodReference>,
    fields: Vec<FieldReference>,
}

/// Exact-owner metadata registry. Registration is atomic and rejects duplicate
/// class descriptors; it does not yet implement superclass or virtual dispatch.
#[derive(Debug, Default)]
pub struct ClassRegistry {
    classes: HashMap<String, ClassDefinition>,
    dex_tables: Vec<DexSymbolTables>,
}

impl ClassRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register_dex(&mut self, inventory: ClassInventory) -> Result<usize, LinkError> {
        let mut incoming = HashSet::new();
        for class in &inventory.classes {
            if self.classes.contains_key(&class.descriptor)
                || !incoming.insert(class.descriptor.clone()) {
                return Err(LinkError::DuplicateClass(class.descriptor.clone()));
            }
        }
        let dex_index = self.dex_tables.len();
        for class in inventory.classes {
            self.classes.insert(class.descriptor.clone(), class);
        }
        self.dex_tables.push(DexSymbolTables {
            methods: inventory.method_references,
            fields: inventory.field_references,
        });
        Ok(dex_index)
    }

    pub fn resolve_declared_field(
        &self, class_descriptor: &str, name: &str, field_type: &str,
    ) -> Result<ResolvedField, LinkError> {
        let class = self.classes.get(class_descriptor)
            .ok_or_else(|| LinkError::ClassNotFound(class_descriptor.to_owned()))?;
        let field = class.fields.iter()
            .find(|field| field.name == name && field.field_type == field_type)
            .cloned()
            .ok_or_else(|| LinkError::FieldNotFound {
                class: class_descriptor.to_owned(), name: name.to_owned(),
                field_type: field_type.to_owned(),
            })?;
        Ok(ResolvedField { class: class.clone(), field })
    }

    /// Resolves a field_id against the exact declaring class. Inherited fields and
    /// framework fields intentionally remain unresolved until class-path support exists.
    pub fn resolve_integer_field_reference(
        &self, dex_index: usize, field_index: usize,
    ) -> Result<ResolvedField, LinkError> {
        let resolved = self.resolve_field_reference(dex_index, field_index)?;
        if !resolved.supports_integer_storage() {
            return Err(LinkError::UnsupportedFieldType(resolved.field.field_type.clone()));
        }
        Ok(resolved)
    }

    pub fn resolve_field_reference(
        &self, dex_index: usize, field_index: usize,
    ) -> Result<ResolvedField, LinkError> {
        let reference = self.dex_tables.get(dex_index)
            .ok_or(LinkError::InvalidDexIndex)?
            .fields.get(field_index)
            .ok_or(LinkError::InvalidFieldIndex)?;
        self.resolve_declared_field(
            &reference.class_descriptor, &reference.name, &reference.field_type,
        )
    }

    pub fn class_count(&self) -> usize { self.classes.len() }
    pub fn dex_count(&self) -> usize { self.dex_tables.len() }

    pub fn field_reference_count(&self, dex_index: usize) -> Result<usize, LinkError> {
        self.dex_tables.get(dex_index)
            .map(|tables| tables.fields.len())
            .ok_or(LinkError::InvalidDexIndex)
    }

    pub fn resolve_declared_method(
        &self, class_descriptor: &str, name: &str, prototype: &str,
    ) -> Result<ResolvedMethod, LinkError> {
        let class = self.classes.get(class_descriptor)
            .ok_or_else(|| LinkError::ClassNotFound(class_descriptor.to_owned()))?;
        let method = class.methods.iter()
            .find(|method| method.name == name && method.prototype == prototype)
            .cloned()
            .ok_or_else(|| LinkError::MethodNotFound {
                class: class_descriptor.to_owned(),
                name: name.to_owned(),
                prototype: prototype.to_owned(),
            })?;
        Ok(ResolvedMethod { class: class.clone(), method })
    }

    /// Resolves one method_id from a registered DEX. Framework/library references
    /// remain unresolved until their class definitions exist in a boot class path.
    pub fn resolve_method_reference(
        &self, dex_index: usize, method_index: usize,
    ) -> Result<ResolvedMethod, LinkError> {
        let reference = self.dex_tables.get(dex_index)
            .ok_or(LinkError::InvalidDexIndex)?
            .methods.get(method_index)
            .ok_or(LinkError::InvalidMethodIndex)?;
        self.resolve_declared_method(
            &reference.class_descriptor, &reference.name, &reference.prototype,
        )
    }
}


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

/// A supplied method body. Incoming arguments are one-word integer values only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexMethodBody {
    pub code_units: Vec<u16>,
    pub register_count: usize,
    pub incoming_words: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    EmptyMethod,
    InvalidRegister(u16),
    TruncatedInstruction,
    UnsupportedOpcode(u8),
    InvalidBranchTarget,
    DivisionByZero,
    StepLimitExceeded,
    NullObjectReference,
    MissingMethodBody(u16),
    ArgumentCountMismatch { expected: usize, actual: usize },
    MissingInvokeResult,
    CallDepthExceeded,
    MissingClassType(u16),
    MissingVirtualMethod { class: String, method_index: u16 },
    InvalidObjectId,
    ObjectIdExhausted,
}

fn width(op: u8) -> Option<usize> {
    match op {
        0x00 | 0x01 | 0x0a | 0x0e | 0x0f | 0x12 | 0x28 | 0xb0..=0xb4 => Some(1),
        0x02 | 0x13 | 0x15 | 0x22 | 0x29 | 0x32..=0x3d | 0x52 | 0x59 | 0x60 | 0x67 | 0x90..=0x94 => Some(2),
        0x6e | 0x70 | 0x71 | 0x74 | 0x76 | 0x77 => Some(3),
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
    source: usize,    delta: isize,    boundaries: &[bool],
) -> Result<usize, VmError> {
    let target = (source as isize)
        .checked_add(delta)
        .ok_or(VmError::InvalidBranchTarget)?;
    if target < 0 || target as usize >= boundaries.len() || !boundaries[target as usize] {
        return Err(VmError::InvalidBranchTarget);
    }
    Ok(target as usize)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IntegerFieldStore {
    static_values: HashMap<u16, i32>,
    instance_values: HashMap<(i32, u16), i32>,
}

impl IntegerFieldStore {
    pub fn get_static(&self, field_index: u16) -> Option<i32> {
        self.static_values.get(&field_index).copied()
    }
    pub fn set_static(&mut self, field_index: u16, value: i32) {
        self.static_values.insert(field_index, value);
    }
    pub fn get_instance(&self, object_id: i32, field_index: u16) -> Option<i32> {
        self.instance_values.get(&(object_id, field_index)).copied()
    }
    pub fn set_instance(&mut self, object_id: i32, field_index: u16, value: i32) {
        self.instance_values.insert((object_id, field_index), value);
    }
}

/// Opaque object handles with exact runtime class descriptors; not a GC heap.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DexObjectStore {
    next_id: i32,
    class_descriptors: HashMap<u16, String>,
    objects: HashMap<i32, String>,
}
impl DexObjectStore {
    pub fn register_class_type(&mut self, type_index: u16, descriptor: impl Into<String>) {
        self.class_descriptors.insert(type_index, descriptor.into());
    }
    pub fn allocate(&mut self, type_index: u16) -> Result<i32, VmError> {
        let descriptor = self.class_descriptors.get(&type_index).cloned()
            .ok_or(VmError::MissingClassType(type_index))?;
        self.next_id = self.next_id.checked_add(1).ok_or(VmError::ObjectIdExhausted)?;
        let id = self.next_id;
        self.objects.insert(id, descriptor);
        Ok(id)
    }
    pub fn class_of(&self, object_id: i32) -> Result<&str, VmError> {
        self.objects.get(&object_id).map(String::as_str).ok_or(VmError::InvalidObjectId)
    }
    pub fn contains(&self, object_id: i32) -> bool { self.objects.contains_key(&object_id) }
}

/// Exact-runtime-class virtual dispatch. Superclass/interface lookup is not implemented.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DexVirtualDispatch {
    implementations: HashMap<(String, u16), u16>,
}
impl DexVirtualDispatch {
    pub fn register(&mut self, runtime_class: impl Into<String>, method_reference: u16, body_id: u16) {
        self.implementations.insert((runtime_class.into(), method_reference), body_id);
    }
    fn resolve(&self, runtime_class: &str, method_reference: u16) -> Option<u16> {
        self.implementations.get(&(runtime_class.to_owned(), method_reference)).copied()
    }
}

/// Executes one already-selected DEX method body represented as little-endian code units.
///
/// `initial_registers` fills the lowest-numbered registers. All remaining registers start
/// at zero. The caller is responsible for selecting a method and preparing its arguments.
pub fn execute_integer_method(
    code_units: &[u16], register_count: usize, initial_registers: &[i32], step_limit: usize,
) -> Result<ExecutionReport, VmError> {
    execute_integer_method_with_fields(code_units, register_count, initial_registers, step_limit, &mut IntegerFieldStore::default())
}

/// Executes the supported integer subset with explicit static and instance field storage.
/// Field indices are DEX field_id indices; callers must resolve and type-check them first.
pub fn execute_integer_method_with_fields(
    code_units: &[u16],
    register_count: usize,
    initial_registers: &[i32],
    step_limit: usize,
    fields: &mut IntegerFieldStore,
) -> Result<ExecutionReport, VmError> {
    execute_integer_method_internal(
        code_units, register_count, initial_registers, step_limit, fields,
        &mut DexObjectStore::default(), &DexVirtualDispatch::default(),
        &HashMap::new(), 0, false,
    )
}

/// Executes one entry method and only the method bodies explicitly supplied by the caller.
/// Incoming arguments are placed in the last registers of each callee frame.
pub fn execute_integer_method_with_calls(
    entry_method: u16,
    methods: &HashMap<u16, DexMethodBody>,
    initial_registers: &[i32],
    step_limit: usize,
    fields: &mut IntegerFieldStore,
) -> Result<ExecutionReport, VmError> {
    let body = methods.get(&entry_method).ok_or(VmError::MissingMethodBody(entry_method))?;
    if initial_registers.len() != body.incoming_words {
        return Err(VmError::ArgumentCountMismatch {
            expected: body.incoming_words, actual: initial_registers.len(),
        });
    }
    execute_integer_method_with_runtime(
        entry_method, methods, initial_registers, step_limit, fields,
        &mut DexObjectStore::default(), &DexVirtualDispatch::default(),
    )
}

/// Execute with explicit object identities and exact-runtime-class virtual dispatch.
pub fn execute_integer_method_with_runtime(
    entry_method: u16,
    methods: &HashMap<u16, DexMethodBody>,
    initial_registers: &[i32],
    step_limit: usize,
    fields: &mut IntegerFieldStore,
    objects: &mut DexObjectStore,
    dispatch: &DexVirtualDispatch,
) -> Result<ExecutionReport, VmError> {
    let body = methods.get(&entry_method).ok_or(VmError::MissingMethodBody(entry_method))?;
    if initial_registers.len() != body.incoming_words {
        return Err(VmError::ArgumentCountMismatch {
            expected: body.incoming_words, actual: initial_registers.len(),
        });
    }
    execute_integer_method_internal(
        &body.code_units, body.register_count, initial_registers, step_limit,
        fields, objects, dispatch, methods, 0, true,
    )
}

fn execute_integer_method_internal(
    code_units: &[u16],
    register_count: usize,
    initial_registers: &[i32],
    step_limit: usize,
    fields: &mut IntegerFieldStore,
    objects: &mut DexObjectStore,
    dispatch: &DexVirtualDispatch,
    methods: &HashMap<u16, DexMethodBody>,
    depth: usize,
    incoming_at_end: bool,
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
    let register_start = if incoming_at_end {
        register_count.checked_sub(initial_registers.len())
            .ok_or(VmError::ArgumentCountMismatch { expected: register_count, actual: initial_registers.len() })?
    } else { 0 };
    registers[register_start..register_start + initial_registers.len()]
        .copy_from_slice(initial_registers);
    let mut pc = 0usize;
    let mut steps = 0usize;
    let mut pending_result: Option<ReturnValue> = None;

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
        if op != 0x0a { pending_result = None; }

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
                pc += 3;            }
            0x0a => {                match pending_result.take() {
                    Some(ReturnValue::Int(value)) => write_register(&mut registers, a8, value)?,
                    _ => return Err(VmError::MissingInvokeResult),
                }
                pc += 1;
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
            0x22 => { // new-instance vAA, type@BBBB
                let type_index = code_units[pc + 1];
                let object_id = objects.allocate(type_index)?;
                write_register(&mut registers, a8, object_id)?;
                pc += 2;
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
            0x6e | 0x70 | 0x71 | 0x74 | 0x76 | 0x77 => { // virtual/direct/static calls
                if depth >= 64 { return Err(VmError::CallDepthExceeded); }
                let method_index = code_units[pc + 1];
                let is_range = op == 0x74 || op == 0x76 || op == 0x77;
                let is_virtual = op == 0x6e || op == 0x74;
                let is_instance = op == 0x6e || op == 0x70 || op == 0x74 || op == 0x76;
                let count;
                let argument_registers: Vec<u16>;
                if !is_range {
                    count = ((first >> 12) & 0x0f) as usize;
                    if count > 5 { return Err(VmError::ArgumentCountMismatch { expected: 5, actual: count }); }
                    let packed = code_units[pc + 2];
                    let candidates = [
                        packed & 0x0f,
                        (packed >> 4) & 0x0f,
                        (packed >> 8) & 0x0f,
                        (packed >> 12) & 0x0f,
                        ((first >> 8) & 0x0f) as u16,
                    ];
                    argument_registers = candidates[..count].to_vec();
                } else {
                    count = a8 as usize;
                    let start = code_units[pc + 2];
                    if count > 0 {
                        let end = (start as usize).checked_add(count)
                            .ok_or(VmError::InvalidRegister(start))?;
                        if end > registers.len() {
                            return Err(VmError::InvalidRegister((end - 1).min(u16::MAX as usize) as u16));
                        }
                    }
                    argument_registers = (0..count).map(|offset| start + offset as u16).collect();
                }
                let target_body = if is_virtual {
                    let receiver = argument_registers.first()
                        .map(|index| register(&registers, *index))
                        .transpose()?.unwrap_or(0);
                    if receiver == 0 { return Err(VmError::NullObjectReference); }
                    let runtime_class = objects.class_of(receiver)?;
                    dispatch.resolve(runtime_class, method_index).ok_or_else(|| {
                        VmError::MissingVirtualMethod { class: runtime_class.to_owned(), method_index }
                    })?
                } else { method_index };
                let callee = methods.get(&target_body)
                    .ok_or(VmError::MissingMethodBody(target_body))?;
                if count != callee.incoming_words {
                    return Err(VmError::ArgumentCountMismatch {
                        expected: callee.incoming_words, actual: count,
                    });
                }
                let mut arguments = Vec::with_capacity(count);
                for argument_register in argument_registers {
                    arguments.push(register(&registers, argument_register)?);
                }
                if is_instance && arguments.first().copied().unwrap_or(0) == 0 {
                    return Err(VmError::NullObjectReference);
                }
                let remaining = step_limit.saturating_sub(steps);
                if remaining == 0 { return Err(VmError::StepLimitExceeded); }
                let report = execute_integer_method_internal(
                    &callee.code_units, callee.register_count, &arguments, remaining,
                    fields, objects, dispatch, methods, depth + 1, true,
                )?;
                steps = steps.checked_add(report.steps).ok_or(VmError::StepLimitExceeded)?;
                if steps > step_limit { return Err(VmError::StepLimitExceeded); }
                pending_result = Some(report.value);
                pc += 3;
            }
            0x52 => { // iget vA, vB, field@CCCC (integer-like fields only)
                let object = register(&registers, b4)?;
                if object == 0 { return Err(VmError::NullObjectReference); }
                let field = code_units[pc + 1];
                let value = fields.get_instance(object, field).unwrap_or(0);
                write_register(&mut registers, a4, value)?;
                pc += 2;
            }
            0x59 => { // iput vA, vB, field@CCCC (integer-like fields only)
                let object = register(&registers, b4)?;
                if object == 0 { return Err(VmError::NullObjectReference); }
                let field = code_units[pc + 1];
                let value = register(&registers, a4)?;
                fields.set_instance(object, field, value);
                pc += 2;
            }
            0x60 => { // sget vAA, field@BBBB (integer-like fields only)
                let field = code_units[pc + 1];
                let value = fields.get_static(field).unwrap_or(0);
                write_register(&mut registers, a8, value)?;
                pc += 2;
            }
            0x67 => { // sput vAA, field@BBBB (integer-like fields only)
                let field = code_units[pc + 1];
                fields.set_static(field, register(&registers, a8)?);
                pc += 2;
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
mod tests {    use super::*;

    fn sample_inventory() -> ClassInventory {
        ClassInventory {
            classes: vec![ClassDefinition {
                class_index: 0,
                descriptor: "Lgame/Main;".into(),
                access_flags: 1,
                superclass_index: None,
                static_field_count: 0,
                instance_field_count: 0,
                direct_method_count: 1,
                virtual_method_count: 0,
                fields: vec![
                    FieldDefinition {
                        field_index: 0, name: "score".into(), field_type: "I".into(),
                        access_flags: 0x0008, is_static: true,
                    },
                    FieldDefinition {
                        field_index: 1, name: "lives".into(), field_type: "I".into(),
                        access_flags: 0x0002, is_static: false,
                    },
                ],
                methods: vec![MethodDefinition {
                    method_index: 0, name: "start".into(), prototype: "()V".into(),
                    access_flags: 0x0009, code_offset: 0, is_direct: true,
                }],
            }],
            method_references: vec![
                MethodReference {
                    method_index: 0, class_descriptor: "Lgame/Main;".into(),
                    name: "start".into(), prototype: "()V".into(),
                },
                MethodReference {
                    method_index: 1, class_descriptor: "Landroid/app/Activity;".into(),
                    name: "onCreate".into(), prototype: "(Landroid/os/Bundle;)V".into(),
                },
            ],
            field_references: vec![
                FieldReference {
                    field_index: 0, class_descriptor: "Lgame/Main;".into(),
                    name: "score".into(), field_type: "I".into(),
                },
                FieldReference {
                    field_index: 1, class_descriptor: "Lgame/Main;".into(),
                    name: "lives".into(), field_type: "I".into(),
                },
            ],
        }
    }

    #[test]
    fn resolves_defined_method_reference_from_registered_dex() {
        let mut registry = ClassRegistry::new();
        let dex = registry.register_dex(sample_inventory()).unwrap();
        let resolved = registry.resolve_method_reference(dex, 0).unwrap();
        assert_eq!(resolved.class.descriptor, "Lgame/Main;");
        assert_eq!(resolved.method.name, "start");
    }

    #[test]
    fn leaves_framework_reference_unresolved_without_boot_class_path() {
        let mut registry = ClassRegistry::new();
        let dex = registry.register_dex(sample_inventory()).unwrap();
        assert_eq!(
            registry.resolve_method_reference(dex, 1),
            Err(LinkError::ClassNotFound("Landroid/app/Activity;".into()))
        );
    }

    #[test]
    fn rejects_duplicate_class_registration_atomically() {
        let mut registry = ClassRegistry::new();
        registry.register_dex(sample_inventory()).unwrap();
        assert_eq!(
            registry.register_dex(sample_inventory()),
            Err(LinkError::DuplicateClass("Lgame/Main;".into()))
        );
        assert_eq!(registry.dex_count(), 1);
        assert_eq!(registry.class_count(), 1);
    }

    #[test]
    fn reports_missing_method_and_invalid_symbol_indices() {
        let mut registry = ClassRegistry::new();
        let dex = registry.register_dex(sample_inventory()).unwrap();
        assert!(matches!(
            registry.resolve_declared_method("Lgame/Main;", "missing", "()V"),
            Err(LinkError::MethodNotFound { .. })
        ));
        assert_eq!(registry.resolve_method_reference(dex, 99), Err(LinkError::InvalidMethodIndex));
        assert_eq!(registry.resolve_method_reference(99, 0), Err(LinkError::InvalidDexIndex));
        assert_eq!(registry.field_reference_count(dex), Ok(2));
        let field = registry.resolve_field_reference(dex, 0).unwrap();
        assert_eq!(field.field.name, "score");
        assert!(field.field.is_static);
        let instance_field = registry.resolve_field_reference(dex, 1).unwrap();
        assert_eq!(instance_field.field.name, "lives");
        assert!(!instance_field.field.is_static);
        assert_eq!(registry.resolve_field_reference(dex, 9), Err(LinkError::InvalidFieldIndex));
        assert!(registry.resolve_integer_field_reference(dex, 0).unwrap().supports_integer_storage());
        assert_eq!(registry.resolve_declared_field("Lgame/Main;", "score", "J"),
            Err(LinkError::FieldNotFound { class: "Lgame/Main;".into(), name: "score".into(), field_type: "J".into() }));
    }

    #[test]
    fn requires_exact_method_prototype() {
        let mut registry = ClassRegistry::new();
        registry.register_dex(sample_inventory()).unwrap();
        assert!(matches!(
            registry.resolve_declared_method("Lgame/Main;", "start", "(I)V"),
            Err(LinkError::MethodNotFound { .. })
        ));
    }


    #[test]
    fn integer_field_store_isolated_by_object_identity() {
        let mut fields = IntegerFieldStore::default();
        fields.set_instance(7, 2, 11);
        fields.set_instance(8, 2, 22);
        assert_eq!(fields.get_instance(7, 2), Some(11));
        assert_eq!(fields.get_instance(8, 2), Some(22));
        assert_eq!(fields.get_instance(9, 2), None);
    }

    #[test]
    fn executes_integer_instance_and_static_field_access() {
        let mut store = IntegerFieldStore::default();
        // v0 = object id 7; v1 = 42; iput v1,v0,field@3; iget v2,v0,field@3; return v2
        let instance = [0x7012, 0x0113, 42, 0x0159, 3, 0x0252, 3, 0x020f];
        let report = execute_integer_method_with_fields(&instance, 3, &[], 20, &mut store).unwrap();
        assert_eq!(report.value, ReturnValue::Int(42));
        assert_eq!(store.get_instance(7, 3), Some(42));

        // v0 = 19; sput v0,field@5; sget v1,field@5; return v1
        let static_code = [0x0013, 19, 0x0067, 5, 0x0160, 5, 0x010f];
        let report = execute_integer_method_with_fields(&static_code, 2, &[], 20, &mut store).unwrap();
        assert_eq!(report.value, ReturnValue::Int(19));
        assert_eq!(store.get_static(5), Some(19));
    }

    #[test]
    fn rejects_null_instance_field_access() {
        let mut store = IntegerFieldStore::default();
        assert_eq!(execute_integer_method_with_fields(&[0x0052, 0, 0x000e], 1, &[], 5, &mut store),
            Err(VmError::NullObjectReference));
    }

    #[test]
    fn executes_constant_add_and_return() {
        // const/4 v0, 7; const/4 v1, 5; add-int v2, v0, v1; return v2
        let code = [0x7012, 0x5112, 0x0290, 0x0100, 0x020f];
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
        let code = [0x0228, 0x0013, 0x0000, 0x000e]; // goto +2 targets the const/16 operand
        assert_eq!(
            execute_integer_method(&code, 0, &[], 10),
            Err(VmError::InvalidBranchTarget)
        );
    }

    #[test]
    fn allocates_object_and_dispatches_virtual_call_by_runtime_class() {
        let mut methods = HashMap::new();
        methods.insert(12, DexMethodBody { code_units: vec![0x7012, 0x000f], register_count: 1, incoming_words: 1 });
        methods.insert(20, DexMethodBody {
            code_units: vec![0x0022, 3, 0x106e, 7, 0, 0x010a, 0x010f],
            register_count: 2, incoming_words: 0,
        });
        let mut objects = DexObjectStore::default();
        objects.register_class_type(3, "Lgame/Vehicle;");
        let mut dispatch = DexVirtualDispatch::default();
        dispatch.register("Lgame/Vehicle;", 7, 12);
        let report = execute_integer_method_with_runtime(
            20, &methods, &[], 30, &mut IntegerFieldStore::default(), &mut objects, &dispatch,        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(7));
        assert_eq!(objects.class_of(1), Ok("Lgame/Vehicle;"));
    }
    #[test]
    fn virtual_dispatch_uses_exact_runtime_class_mapping() {
        let mut methods = HashMap::new();
        methods.insert(1, DexMethodBody { code_units: vec![0x7012, 0x000f], register_count: 1, incoming_words: 1 });
        methods.insert(2, DexMethodBody { code_units: vec![0x3012, 0x000f], register_count: 1, incoming_words: 1 });
        methods.insert(3, DexMethodBody {
            code_units: vec![0x0022, 9, 0x106e, 4, 0, 0x010a, 0x010f],
            register_count: 2, incoming_words: 0,
        });
        let mut objects = DexObjectStore::default();
        objects.register_class_type(9, "Lgame/Truck;");
        let mut dispatch = DexVirtualDispatch::default();
        dispatch.register("Lgame/Truck;", 4, 2);
        let report = execute_integer_method_with_runtime(
            3, &methods, &[], 30, &mut IntegerFieldStore::default(), &mut objects, &dispatch,
        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(3));
    }

    #[test]
    fn rejects_missing_virtual_implementation_and_null_receiver() {
        let mut methods = HashMap::new();
        methods.insert(1, DexMethodBody { code_units: vec![0x000e], register_count: 1, incoming_words: 1 });
        methods.insert(2, DexMethodBody { code_units: vec![0x006e, 5, 0, 0x000e], register_count: 1, incoming_words: 0 });
        assert_eq!(
            execute_integer_method_with_runtime(
                2, &methods, &[], 10, &mut IntegerFieldStore::default(),
                &mut DexObjectStore::default(), &DexVirtualDispatch::default(),
            ),
            Err(VmError::NullObjectReference)
        );
        let mut objects = DexObjectStore::default();
        objects.register_class_type(2, "Lgame/Car;");
        let mut create_then_call = HashMap::new();
        create_then_call.insert(2, DexMethodBody {
            code_units: vec![0x0022, 2, 0x106e, 5, 0, 0x000e],
            register_count: 1, incoming_words: 0,
        });
        assert!(matches!(
            execute_integer_method_with_runtime(
                2, &create_then_call, &[], 10, &mut IntegerFieldStore::default(),
                &mut objects, &DexVirtualDispatch::default(),
            ),
            Err(VmError::MissingVirtualMethod { .. })
        ));
    }

    #[test]
    fn rejects_unknown_class_type_during_object_allocation() {
        let mut methods = HashMap::new();
        methods.insert(1, DexMethodBody {
            code_units: vec![0x0022, 77, 0x000e], register_count: 1, incoming_words: 0,
        });
        assert_eq!(
            execute_integer_method_with_runtime(
                1, &methods, &[], 10, &mut IntegerFieldStore::default(),
                &mut DexObjectStore::default(), &DexVirtualDispatch::default(),
            ),
            Err(VmError::MissingClassType(77))
        );
    }

    #[test]
    fn invokes_static_method_with_35c_arguments_and_move_result() {
        let mut methods = HashMap::new();
        methods.insert(0, DexMethodBody {
            code_units: vec![0x5012, 0x0090, 0x0001, 0x000f],
            register_count: 2, incoming_words: 1,
        });
        methods.insert(7, DexMethodBody {
            code_units: vec![0x0013, 7, 0x1071, 0, 0, 0x010a, 0x010f],
            register_count: 2, incoming_words: 0,
        });
        let report = execute_integer_method_with_calls(
            7, &methods, &[], 30, &mut IntegerFieldStore::default(),
        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(12));
        assert_eq!(report.steps, 7);
    }

    #[test]
    fn invokes_direct_instance_method_and_shares_object_fields() {
        let mut methods = HashMap::new();
        methods.insert(4, DexMethodBody {
            // Incoming receiver is v1; read its field and return the value.
            code_units: vec![0x1052, 0, 0x000f],
            register_count: 2, incoming_words: 1,
        });
        methods.insert(8, DexMethodBody {
            // v0 = object id 42; invoke-direct {v0}, method@4; move-result v1.
            code_units: vec![0x0013, 42, 0x1070, 4, 0, 0x010a, 0x010f],
            register_count: 2, incoming_words: 0,
        });
        let mut fields = IntegerFieldStore::default();
        fields.set_instance(42, 0, 123);
        let report = execute_integer_method_with_calls(8, &methods, &[], 30, &mut fields).unwrap();
        assert_eq!(report.value, ReturnValue::Int(123));
    }

    #[test]
    fn rejects_null_receiver_for_invoke_direct() {
        let mut methods = HashMap::new();
        methods.insert(4, DexMethodBody {
            code_units: vec![0x000e], register_count: 1, incoming_words: 1,
        });
        methods.insert(8, DexMethodBody {
            code_units: vec![0x1070, 4, 0, 0x000e],
            register_count: 1, incoming_words: 0,
        });
        assert_eq!(
            execute_integer_method_with_calls(8, &methods, &[], 20, &mut IntegerFieldStore::default()),
            Err(VmError::NullObjectReference)
        );
    }

    #[test]
    fn invokes_direct_range_method_with_receiver_and_argument() {
        let mut methods = HashMap::new();
        methods.insert(4, DexMethodBody {
            // v1 + v2 -> v0
            code_units: vec![0x0090, 0x0201, 0x000f],
            register_count: 3, incoming_words: 2,
        });
        methods.insert(8, DexMethodBody {
            // v0=9, v1=5, invoke-direct/range {v0..v1}, method@4; move-result v2
            code_units: vec![0x0013, 9, 0x0113, 5, 0x0276, 4, 0, 0x020a, 0x020f],
            register_count: 3, incoming_words: 0,
        });
        let report = execute_integer_method_with_calls(
            8, &methods, &[], 30, &mut IntegerFieldStore::default(),
        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(14));
    }

    #[test]
    fn invokes_static_range_method_with_contiguous_arguments() {
        let mut methods = HashMap::new();
        methods.insert(2, DexMethodBody {
            code_units: vec![0x0090, 0x0001, 0x000f],
            register_count: 2, incoming_words: 2,
        });
        methods.insert(9, DexMethodBody {
            code_units: vec![0x0013, 4, 0x0113, 6, 0x0277, 2, 0, 0x020a, 0x020f],
            register_count: 3, incoming_words: 0,
        });
        let report = execute_integer_method_with_calls(
            9, &methods, &[], 30, &mut IntegerFieldStore::default(),
        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(10));
    }

    #[test]
    fn rejects_missing_callee_and_argument_shape_mismatch() {
        let mut methods = HashMap::new();
        methods.insert(1, DexMethodBody {
            code_units: vec![0x000e], register_count: 0, incoming_words: 0,
        });
        methods.insert(3, DexMethodBody {
            code_units: vec![0x1071, 1, 0, 0x000e],
            register_count: 1, incoming_words: 0,
        });
        assert_eq!(
            execute_integer_method_with_calls(3, &methods, &[], 10, &mut IntegerFieldStore::default()),
            Err(VmError::ArgumentCountMismatch { expected: 0, actual: 1 })
        );
        assert_eq!(
            execute_integer_method_with_calls(99, &methods, &[], 10, &mut IntegerFieldStore::default()),
            Err(VmError::MissingMethodBody(99))
        );
    }

    #[test]
    fn rejects_move_result_without_immediately_preceding_invoke() {
        assert_eq!(
            execute_integer_method(&[0x000a, 0x000e], 1, &[], 10),
            Err(VmError::MissingInvokeResult)
        );
    }

    #[test]
    fn enforces_call_depth_limit() {
        let mut methods = HashMap::new();
        methods.insert(1, DexMethodBody {
            code_units: vec![0x0071, 1, 0, 0x000e],
            register_count: 0, incoming_words: 0,
        });
        assert_eq!(
            execute_integer_method_with_calls(1, &methods, &[], 500, &mut IntegerFieldStore::default()),
            Err(VmError::CallDepthExceeded)
        );
    }

    #[test]
    fn rejects_still_unsupported_invoke_super_opcode() {
        let code = [0x006f];
        assert_eq!(
            execute_integer_method(&code, 1, &[], 10),
            Err(VmError::UnsupportedOpcode(0x6f))
        );
    }
}


// DEX-to-interpreter bridge. This loads code_item bodies from a parsed class
// inventory; it does not imply that Android framework or native methods exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexLoadError {
    InvalidDex(stride_dex_inspector::classes::ClassReadError),
    InvalidClassTypeIndex(u32),
    InvalidMethodIndex(u32),
    DuplicateClassType(u16),
    DuplicateMethodBody(u16),
    CodeItem { method_index: u32, error: stride_dex_inspector::CodeItemError },
}

#[derive(Debug, Default)]
pub struct DexExecutable {
    methods: HashMap<u16, DexMethodBody>,
    objects: DexObjectStore,
    dispatch: DexVirtualDispatch,
    class_count: usize,
}

impl DexExecutable {
    /// Parse a complete DEX buffer and load its defined method bodies.
    /// This connects DEX metadata inspection directly to the bounded interpreter.
    pub fn from_dex_bytes(dex_bytes: &[u8]) -> Result<Self, DexLoadError> {
        let inventory = stride_dex_inspector::classes::inspect_classes(dex_bytes)
            .map_err(DexLoadError::InvalidDex)?;
        Self::from_inventory(dex_bytes, inventory)
    }

    /// Build executable method bodies from one DEX byte buffer and its parsed inventory.
    /// Abstract/native methods (code_off == 0) are deliberately left unresolved.
    pub fn from_inventory(
        dex_bytes: &[u8],
        inventory: ClassInventory,
    ) -> Result<Self, DexLoadError> {
        let mut executable = Self::default();
        let mut seen_types = HashSet::new();

        for class in &inventory.classes {
            let type_index = u16::try_from(class.class_index)
                .map_err(|_| DexLoadError::InvalidClassTypeIndex(class.class_index))?;
            if !seen_types.insert(type_index) {
                return Err(DexLoadError::DuplicateClassType(type_index));
            }
            executable.objects.register_class_type(type_index, class.descriptor.clone());
            executable.class_count += 1;
        }

        for class in &inventory.classes {
            for method in &class.methods {
                if method.code_offset == 0 {
                    continue;
                }
                let method_id = u16::try_from(method.method_index)
                    .map_err(|_| DexLoadError::InvalidMethodIndex(method.method_index))?;
                if executable.methods.contains_key(&method_id) {
                    return Err(DexLoadError::DuplicateMethodBody(method_id));
                }
                let code = stride_dex_inspector::read_code_item(dex_bytes, method.code_offset)
                    .map_err(|error| DexLoadError::CodeItem {
                        method_index: method.method_index,
                        error,
                    })?;
                let code_units = code.instructions
                    .chunks_exact(2)
                    .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
                    .collect();
                executable.methods.insert(method_id, DexMethodBody {
                    code_units,
                    register_count: usize::from(code.registers_size),
                    incoming_words: usize::from(code.ins_size),
                });
                if !method.is_direct {
                    executable.dispatch.register(
                        class.descriptor.clone(),
                        method_id,
                        method_id,
                    );
                }
            }
        }
        Ok(executable)
    }

    pub fn class_count(&self) -> usize { self.class_count }
    pub fn method_count(&self) -> usize { self.methods.len() }
    pub fn has_method_body(&self, method_id: u16) -> bool {
        self.methods.contains_key(&method_id)
    }

    /// Execute a loaded method with the existing bounded integer interpreter.
    /// This is not a complete Android app launch path.
    pub fn execute_entry(
        &mut self,
        method_id: u16,
        arguments: &[i32],
        step_limit: usize,
        fields: &mut IntegerFieldStore,
    ) -> Result<ExecutionReport, VmError> {
        execute_integer_method_with_runtime(
            method_id,
            &self.methods,
            arguments,
            step_limit,
            fields,
            &mut self.objects,
            &self.dispatch,
        )
    }
}

#[cfg(test)]
mod dex_loader_tests {
    use super::*;

    fn one_method_inventory(code_offset: u32) -> ClassInventory {
        ClassInventory {
            classes: vec![ClassDefinition {
                class_index: 0,
                descriptor: "Lsample/Entry;".into(),
                access_flags: 1,
                superclass_index: None,
                static_field_count: 0,
                instance_field_count: 0,
                direct_method_count: 1,
                virtual_method_count: 0,
                fields: vec![],
                methods: vec![MethodDefinition {
                    method_index: 0,
                    name: "entry".into(),
                    prototype: "()I".into(),
                    access_flags: 0x0009,
                    code_offset,
                    is_direct: true,
                }],
            }],
            method_references: vec![],
            field_references: vec![],
        }
    }

    fn minimal_code_item() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x70 + 16 + 4];
        let offset = 0x70;
        bytes[offset..offset + 2].copy_from_slice(&1u16.to_le_bytes());
        bytes[offset + 2..offset + 4].copy_from_slice(&0u16.to_le_bytes());
        bytes[offset + 12..offset + 16].copy_from_slice(&2u32.to_le_bytes());
        // const/4 v0, 5; return v0
        bytes[offset + 16..offset + 18].copy_from_slice(&0x5012u16.to_le_bytes());
        bytes[offset + 18..offset + 20].copy_from_slice(&0x000fu16.to_le_bytes());
        bytes
    }

    #[test]
    fn loads_code_item_and_executes_entry_method() {
        let bytes = minimal_code_item();
        let mut executable = DexExecutable::from_inventory(
            &bytes,
            one_method_inventory(0x70),
        ).unwrap();
        assert_eq!(executable.class_count(), 1);
        assert_eq!(executable.method_count(), 1);
        assert!(executable.has_method_body(0));
        let report = executable.execute_entry(
            0, &[], 10, &mut IntegerFieldStore::default(),
        ).unwrap();
        assert_eq!(report.value, ReturnValue::Int(5));
    }

    #[test]
    fn rejects_invalid_dex_before_loading_any_method() {
        assert!(matches!(
            DexExecutable::from_dex_bytes(&[0u8; 16]),
            Err(DexLoadError::InvalidDex(_))
        ));
    }

    #[test]
    fn skips_abstract_or_native_methods_without_code_items() {
        let executable = DexExecutable::from_inventory(
            &minimal_code_item(),
            one_method_inventory(0),
        ).unwrap();
        assert_eq!(executable.class_count(), 1);
        assert_eq!(executable.method_count(), 0);
        assert!(!executable.has_method_body(0));
    }

    #[test]
    fn rejects_invalid_code_item_offsets_without_partial_executable() {
        let result = DexExecutable::from_inventory(
            &minimal_code_item(),
            one_method_inventory(u32::MAX),
        );
        assert!(matches!(
            result,
            Err(DexLoadError::CodeItem { method_index: 0, .. })
        ));
    }
}