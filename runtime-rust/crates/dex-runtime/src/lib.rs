//! A deliberately small, bounds-checked Dalvik integer interpreter.
//!
//! This is an execution primitive, not ART. It supports a limited integer opcode
//! subset and a metadata-only class/method registry. It does not provide Android
//! framework classes, inheritance dispatch, JNI, or native library execution.

use std::collections::{HashMap, HashSet};
use stride_dex_inspector::{
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    EmptyMethod,
    InvalidRegister(u16),
    TruncatedInstruction,
    UnsupportedOpcode(u8),
    InvalidBranchTarget,
    DivisionByZero,
    StepLimitExceeded,
    NullObjectReference,
}

fn width(op: u8) -> Option<usize> {
    match op {
        0x00 | 0x01 | 0x0e | 0x0f | 0x12 | 0x28 | 0xb0..=0xb4 => Some(1),
        0x02 | 0x13 | 0x15 | 0x29 | 0x32..=0x3d | 0x52 | 0x59 | 0x60 | 0x67 | 0x90..=0x94 => Some(2),
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
mod tests {
    use super::*;

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
        assert_eq!(registry.field_reference_count(dex), Ok(1));
        let field = registry.resolve_field_reference(dex, 0).unwrap();
        assert_eq!(field.field.name, "score");
        assert!(field.field.is_static);
        let instance_field = registry.resolve_field_reference(dex, 1).unwrap();
        assert_eq!(instance_field.field.name, "lives");
        assert!(!instance_field.field.is_static);
        assert_eq!(registry.resolve_field_reference(dex, 9), Err(LinkError::InvalidFieldIndex));
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
    fn executes_integer_instance_and_static_field_access() {
        let mut store = IntegerFieldStore::default();
        // v0 = object id 7; v1 = 42; iput v1,v0,field@3; iget v2,v0,field@3; return v2
        let instance = [0x0012, 7, 0x1012, 42, 0x1059, 3, 0x2052, 3, 0x000f];
        let report = execute_integer_method_with_fields(&instance, 3, &[], 20, &mut store).unwrap();
        assert_eq!(report.value, ReturnValue::Int(42));
        assert_eq!(store.get_instance(7, 3), Some(42));

        // v0 = 19; sput v0,field@5; sget v1,field@5; return v1
        let static_code = [0x0012, 19, 0x0067, 5, 0x0160, 5, 0x000f];
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
    fn rejects_unsupported_object_and_invoke_opcodes() {
        let code = [0x006e, 0x0000, 0x0000];
        assert_eq!(
            execute_integer_method(&code, 1, &[], 10),
            Err(VmError::UnsupportedOpcode(0x6e))
        );
    }
}
