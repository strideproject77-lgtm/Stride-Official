//! Bounds-checked discovery of DEX classes and encoded methods.
//! This is metadata parsing only; it does not verify or execute Android code.

use crate::{read_code_item, CodeItemError};

const HEADER: usize = 0x70;
const NO_INDEX: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassReadError {
    TruncatedHeader,
    InvalidMagic,
    InvalidFileSize,
    InvalidHeader,
    InvalidTable,
    InvalidIndex,
    InvalidClassData,
    InvalidUleb128,
    InvalidCodeItem,
    InvalidInstructions,
    InvalidRegisterLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDefinition {
    pub method_index: u32,
    pub name: String,
    pub prototype: String,
    pub access_flags: u32,
    pub code_offset: u32,
    pub is_direct: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDefinition {
    pub class_index: u32,
    pub descriptor: String,
    pub access_flags: u32,
    pub superclass_index: Option<u32>,
    pub static_field_count: u32,
    pub instance_field_count: u32,
    pub direct_method_count: u32,
    pub virtual_method_count: u32,
    pub methods: Vec<MethodDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassInventory {
    pub classes: Vec<ClassDefinition>,
}

fn u16le(b: &[u8], o: usize) -> Result<u16, ClassReadError> {
    let s = b.get(o..o.checked_add(2).ok_or(ClassReadError::InvalidTable)?)
        .ok_or(ClassReadError::InvalidTable)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32le(b: &[u8], o: usize) -> Result<u32, ClassReadError> {
    let s = b.get(o..o.checked_add(4).ok_or(ClassReadError::InvalidTable)?)
        .ok_or(ClassReadError::InvalidTable)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn table(b: &[u8], count: u32, offset: u32, item_size: usize) -> Result<std::ops::Range<usize>, ClassReadError> {
    let n = usize::try_from(count).map_err(|_| ClassReadError::InvalidTable)?;
    let start = usize::try_from(offset).map_err(|_| ClassReadError::InvalidTable)?;
    if n > 1_000_000 { return Err(ClassReadError::InvalidTable); }
    if n == 0 { return if start <= b.len() { Ok(start..start) } else { Err(ClassReadError::InvalidTable) }; }
    if start < HEADER || start % 4 != 0 { return Err(ClassReadError::InvalidTable); }
    let end = start.checked_add(n.checked_mul(item_size).ok_or(ClassReadError::InvalidTable)?)
        .filter(|e| *e <= b.len()).ok_or(ClassReadError::InvalidTable)?;
    Ok(start..end)
}

fn uleb(b: &[u8], p: &mut usize) -> Result<u32, ClassReadError> {
    let mut value = 0u32;
    for i in 0..5usize {
        let byte = *b.get(*p).ok_or(ClassReadError::InvalidClassData)?;
        *p += 1;
        if i == 4 && byte & 0xf0 != 0 { return Err(ClassReadError::InvalidUleb128); }
        value |= u32::from(byte & 0x7f) << (i * 7);
        if byte & 0x80 == 0 { return Ok(value); }
    }
    Err(ClassReadError::InvalidUleb128)
}

fn delta_index(previous: u32, delta: u32, count: u32) -> Result<u32, ClassReadError> {
    let value = previous.checked_add(delta).ok_or(ClassReadError::InvalidIndex)?;
    if value >= count { return Err(ClassReadError::InvalidIndex); }
    Ok(value)
}


fn dex_string(
    b: &[u8], strings: &std::ops::Range<usize>, string_count: u32, index: u32,
) -> Result<String, ClassReadError> {
    if index >= string_count { return Err(ClassReadError::InvalidIndex); }
    let at = strings.start.checked_add(index as usize * 4).ok_or(ClassReadError::InvalidIndex)?;
    let data_offset = usize::try_from(u32le(b, at)?).map_err(|_| ClassReadError::InvalidClassData)?;
    if data_offset < HEADER || data_offset >= b.len() { return Err(ClassReadError::InvalidClassData); }
    let mut p = data_offset;
    let utf16_length = usize::try_from(uleb(b, &mut p)?).map_err(|_| ClassReadError::InvalidClassData)?;
    if utf16_length > b.len() { return Err(ClassReadError::InvalidClassData); }
    let mut units = Vec::with_capacity(utf16_length);
    loop {
        let first = *b.get(p).ok_or(ClassReadError::InvalidClassData)?;
        p += 1;
        if first == 0 { break; }
        let unit = if first <= 0x7f {
            u16::from(first)
        } else if first & 0xe0 == 0xc0 {
            let second = *b.get(p).ok_or(ClassReadError::InvalidClassData)?;
            p += 1;
            if second & 0xc0 != 0x80 { return Err(ClassReadError::InvalidClassData); }
            let value = (u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f);
            if value != 0 && value < 0x80 { return Err(ClassReadError::InvalidClassData); }
            value
        } else if first & 0xf0 == 0xe0 {
            let second = *b.get(p).ok_or(ClassReadError::InvalidClassData)?;
            let third = *b.get(p + 1).ok_or(ClassReadError::InvalidClassData)?;
            p += 2;
            if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 {
                return Err(ClassReadError::InvalidClassData);
            }
            let value = (u16::from(first & 0x0f) << 12)
                | (u16::from(second & 0x3f) << 6) | u16::from(third & 0x3f);
            if value < 0x800 { return Err(ClassReadError::InvalidClassData); }
            value
        } else {
            return Err(ClassReadError::InvalidClassData);
        };
        units.push(unit);
        if units.len() > utf16_length { return Err(ClassReadError::InvalidClassData); }
    }
    if units.len() != utf16_length { return Err(ClassReadError::InvalidClassData); }
    Ok(String::from_utf16_lossy(&units))
}

fn type_descriptor(
    b: &[u8], strings: &std::ops::Range<usize>, types: &std::ops::Range<usize>,
    string_count: u32, type_count: u32, type_index: u32,
) -> Result<String, ClassReadError> {
    if type_index >= type_count { return Err(ClassReadError::InvalidIndex); }
    let at = types.start.checked_add(type_index as usize * 4).ok_or(ClassReadError::InvalidIndex)?;
    dex_string(b, strings, string_count, u32le(b, at)?)
}

fn proto_signature(
    b: &[u8], strings: &std::ops::Range<usize>, types: &std::ops::Range<usize>,
    protos: &std::ops::Range<usize>, string_count: u32, type_count: u32, proto_index: u32,
) -> Result<String, ClassReadError> {
    if proto_index as usize >= protos.len() / 12 { return Err(ClassReadError::InvalidIndex); }
    let at = protos.start + proto_index as usize * 12;
    let return_type = u32le(b, at + 4)?;
    let parameters_offset = u32le(b, at + 8)?;
    let mut signature = String::from("(");
    if parameters_offset != 0 {
        let list = usize::try_from(parameters_offset).map_err(|_| ClassReadError::InvalidClassData)?;
        if list < HEADER || list % 4 != 0 { return Err(ClassReadError::InvalidClassData); }
        let count = usize::try_from(u32le(b, list)?).map_err(|_| ClassReadError::InvalidClassData)?;
        if count > 1_000_000 || list.checked_add(4).and_then(|v| v.checked_add(count.checked_mul(2)?))
            .filter(|end| *end <= b.len()).is_none() {
            return Err(ClassReadError::InvalidClassData);
        }
        for i in 0..count {
            let type_index = u32::from(u16le(b, list + 4 + i * 2)?);
            signature.push_str(&type_descriptor(b, strings, types, string_count, type_count, type_index)?);
        }
    }
    signature.push(')');
    signature.push_str(&type_descriptor(b, strings, types, string_count, type_count, return_type)?);
    Ok(signature)
}

fn parameter_word_count(prototype: &str) -> Result<u16, ClassReadError> {
    let params = prototype.strip_prefix('(')
        .and_then(|value| value.split_once(')').map(|(args, _)| args))
        .ok_or(ClassReadError::InvalidRegisterLayout)?;
    let bytes = params.as_bytes();
    let mut cursor = 0usize;
    let mut words = 0u16;
    while cursor < bytes.len() {
        let mut dimensions = 0usize;
        while bytes.get(cursor) == Some(&b'[') {
            dimensions += 1;
            cursor += 1;
        }
        let kind = *bytes.get(cursor).ok_or(ClassReadError::InvalidRegisterLayout)?;
        cursor += 1;
        match kind {
            b'L' => {
                let tail = params.get(cursor..).ok_or(ClassReadError::InvalidRegisterLayout)?;
                let end = tail.find(';').ok_or(ClassReadError::InvalidRegisterLayout)?;
                if end == 0 { return Err(ClassReadError::InvalidRegisterLayout); }
                cursor += end + 1;
                let _ = dimensions;
                words = words.checked_add(1).ok_or(ClassReadError::InvalidRegisterLayout)?;
            }
            b'Z' | b'B' | b'C' | b'S' | b'I' | b'F' | b'J' | b'D' => {
                let width = if dimensions > 0 { 1 } else if matches!(kind, b'J' | b'D') { 2 } else { 1 };
                words = words.checked_add(width).ok_or(ClassReadError::InvalidRegisterLayout)?;
            }
            _ => return Err(ClassReadError::InvalidRegisterLayout),
        }
    }
    Ok(words)
}

fn validate_register_layout(
    registers_size: u16,
    ins_size: u16,
    access_flags: u32,
    prototype: &str,
) -> Result<(), ClassReadError> {
    let receiver_words = if access_flags & 0x0008 == 0 { 1u16 } else { 0u16 };
    let expected = parameter_word_count(prototype)?
        .checked_add(receiver_words)
        .ok_or(ClassReadError::InvalidRegisterLayout)?;
    if ins_size != expected || registers_size < ins_size {
        return Err(ClassReadError::InvalidRegisterLayout);
    }
    Ok(())
}

fn class_data(
    b: &[u8], offset: u32, class_index: u32, fields: &std::ops::Range<usize>,
    methods: &std::ops::Range<usize>, strings: &std::ops::Range<usize>,
    types: &std::ops::Range<usize>, protos: &std::ops::Range<usize>,
    field_count: u32, method_count: u32, string_count: u32, type_count: u32,
) -> Result<ClassDefinition, ClassReadError> {
    let mut p = usize::try_from(offset).map_err(|_| ClassReadError::InvalidClassData)?;
    if p < HEADER || p >= b.len() { return Err(ClassReadError::InvalidClassData); }
    let static_fields = uleb(b, &mut p)?;
    let instance_fields = uleb(b, &mut p)?;
    let direct_methods = uleb(b, &mut p)?;
    let virtual_methods = uleb(b, &mut p)?;
    if u64::from(static_fields) + u64::from(instance_fields) > b.len() as u64
        || u64::from(direct_methods) + u64::from(virtual_methods) > b.len() as u64 {
        return Err(ClassReadError::InvalidClassData);
    }

    for count in [static_fields, instance_fields] {
        let mut idx = 0;
        for _ in 0..count {
            idx = delta_index(idx, uleb(b, &mut p)?, field_count)?;
            let _flags = uleb(b, &mut p)?;
            let at = fields.start.checked_add(idx as usize * 8).ok_or(ClassReadError::InvalidIndex)?;
            if at >= fields.end || u32::from(u16le(b, at)?) != class_index {
                return Err(ClassReadError::InvalidIndex);
            }
        }
    }

    let mut found = Vec::new();
    for (count, is_direct) in [(direct_methods, true), (virtual_methods, false)] {
        let mut idx = 0;
        for _ in 0..count {
            idx = delta_index(idx, uleb(b, &mut p)?, method_count)?;
            let flags = uleb(b, &mut p)?;
            let code = uleb(b, &mut p)?;
            let at = methods.start.checked_add(idx as usize * 8).ok_or(ClassReadError::InvalidIndex)?;
            if at >= methods.end || u32::from(u16le(b, at)?) != class_index {
                return Err(ClassReadError::InvalidIndex);
            }
            let proto_index = u32::from(u16le(b, at + 2)?);
            let name_index = u32le(b, at + 4)?;
            let name = dex_string(b, strings, string_count, name_index)?;
            let prototype = proto_signature(b, strings, types, protos, string_count, type_count, proto_index)?;
            if code != 0 {
                let body = read_code_item(b, code)
                    .map_err(|_: CodeItemError| ClassReadError::InvalidCodeItem)?;
                validate_register_layout(body.registers_size, body.ins_size, flags, &prototype)?;
                super::decode_instructions(body.instructions)
                    .map_err(|_| ClassReadError::InvalidInstructions)?;
            }
            found.push(MethodDefinition {
                method_index: idx, name, prototype, access_flags: flags, code_offset: code, is_direct
            });
        }
    }
    Ok(ClassDefinition {
        class_index, descriptor: String::new(), access_flags: 0, superclass_index: None,
        static_field_count: static_fields, instance_field_count: instance_fields,
        direct_method_count: direct_methods, virtual_method_count: virtual_methods,
        methods: found,
    })
}

/// Reads class definitions and their encoded fields/methods after validating DEX table bounds.
pub fn inspect_classes(bytes: &[u8]) -> Result<ClassInventory, ClassReadError> {
    if bytes.len() < HEADER { return Err(ClassReadError::TruncatedHeader); }
    if bytes.get(0..4) != Some(&b"dex\n"[..])
        || !bytes.get(4..7).is_some_and(|v| v.iter().all(u8::is_ascii_digit))
        || bytes[7] != 0 {
        return Err(ClassReadError::InvalidMagic);
    }
    let size = usize::try_from(u32le(bytes, 0x20)?).map_err(|_| ClassReadError::InvalidFileSize)?;
    if size < HEADER || size > bytes.len() { return Err(ClassReadError::InvalidFileSize); }
    let b = &bytes[..size];
    if u32le(b, 0x24)? as usize != HEADER || u32le(b, 0x28)? != 0x1234_5678 {
        return Err(ClassReadError::InvalidHeader);
    }

    let string_count = u32le(b, 0x38)?;
    let type_count = u32le(b, 0x40)?;
    let proto_count = u32le(b, 0x48)?;
    let field_count = u32le(b, 0x50)?;
    let method_count = u32le(b, 0x58)?;
    let class_count = u32le(b, 0x60)?;
    let strings = table(b, string_count, u32le(b, 0x3c)?, 4)?;
    let types = table(b, type_count, u32le(b, 0x44)?, 4)?;
    let protos = table(b, proto_count, u32le(b, 0x4c)?, 12)?;
    let fields = table(b, field_count, u32le(b, 0x54)?, 8)?;
    let methods = table(b, method_count, u32le(b, 0x5c)?, 8)?;
    let defs = table(b, class_count, u32le(b, 0x64)?, 32)?;

    for i in 0..type_count as usize {
        if u32le(b, types.start + i * 4)? >= string_count { return Err(ClassReadError::InvalidIndex); }
    }
    for i in 0..proto_count as usize {
        let at = protos.start + i * 12;
        if u32le(b, at)? >= string_count || u32le(b, at + 4)? >= type_count {
            return Err(ClassReadError::InvalidIndex);
        }
    }
    for i in 0..field_count as usize {
        let at = fields.start + i * 8;
        if u32::from(u16le(b, at)?) >= type_count
            || u32::from(u16le(b, at + 2)?) >= type_count
            || u32le(b, at + 4)? >= string_count { return Err(ClassReadError::InvalidIndex); }
    }
    for i in 0..method_count as usize {
        let at = methods.start + i * 8;
        if u32::from(u16le(b, at)?) >= type_count
            || u32::from(u16le(b, at + 2)?) >= proto_count
            || u32le(b, at + 4)? >= string_count { return Err(ClassReadError::InvalidIndex); }
    }

    let mut classes = Vec::with_capacity(class_count as usize);
    for i in 0..class_count as usize {
        let at = defs.start + i * 32;
        let id = u32le(b, at)?;
        let flags = u32le(b, at + 4)?;
        let super_id = u32le(b, at + 8)?;
        let data_off = u32le(b, at + 24)?;
        if id >= type_count || (super_id != NO_INDEX && super_id >= type_count) {
            return Err(ClassReadError::InvalidIndex);
        }
        let mut class = if data_off == 0 {
            ClassDefinition {
                class_index: id, descriptor: String::new(), access_flags: 0, superclass_index: None,
                static_field_count: 0, instance_field_count: 0,
                direct_method_count: 0, virtual_method_count: 0, methods: Vec::new(),
            }
        } else {
            class_data(
                b, data_off, id, &fields, &methods, &strings, &types, &protos,
                field_count, method_count, string_count, type_count,
            )?
        };
        class.descriptor = type_descriptor(b, &strings, &types, string_count, type_count, id)?;
        class.access_flags = flags;
        class.superclass_index = (super_id != NO_INDEX).then_some(super_id);
        classes.push(class);
    }
    let _ = strings;
    Ok(ClassInventory { classes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(method_owner: u16) -> Vec<u8> {
        let strings = 0x70usize;
        let types = strings + 8;
        let protos = types + 8;
        let methods = protos + 12;
        let defs = methods + 8;
        let data = defs + 32;
        let class_data = [0, 0, 1, 0, 0, 1, 0];
        let string_data = [3, b'L', b'x', b';', 0, 3, b'f', b'o', b'o', 0];
        let mut b = vec![0; data + class_data.len() + string_data.len()];
        b[..8].copy_from_slice(b"dex\n039\0");
        let file_size = b.len() as u32;
        b[0x20..0x24].copy_from_slice(&file_size.to_le_bytes());
        b[0x24..0x28].copy_from_slice(&(HEADER as u32).to_le_bytes());
        b[0x28..0x2c].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        for (at, n, off) in [(0x38,2u32,strings),(0x40,2,types),(0x48,1,protos),(0x58,1,methods),(0x60,1,defs)] {
            b[at..at+4].copy_from_slice(&n.to_le_bytes());
            b[at+4..at+8].copy_from_slice(&(off as u32).to_le_bytes());
        }
        let string_data_at = data + class_data.len();
        b[strings..strings+4].copy_from_slice(&(string_data_at as u32).to_le_bytes());
        b[strings+4..strings+8].copy_from_slice(&((string_data_at + 5) as u32).to_le_bytes());
        b[types..types+8].fill(0);
        b[protos..protos+8].fill(0);
        b[methods..methods+2].copy_from_slice(&method_owner.to_le_bytes());
        b[methods+2..methods+4].fill(0);
        b[methods+4..methods+8].copy_from_slice(&1u32.to_le_bytes());
        b[defs..defs+4].fill(0);
        b[defs+8..defs+12].copy_from_slice(&NO_INDEX.to_le_bytes());
        b[defs+24..defs+28].copy_from_slice(&(data as u32).to_le_bytes());
        b[data..data+class_data.len()].copy_from_slice(&class_data);
        b[data+class_data.len()..].copy_from_slice(&string_data);
        b
    }

    #[test]
    fn reads_class_and_method_metadata() {
        let inventory = inspect_classes(&sample(0)).unwrap();
        let class = &inventory.classes[0];
        assert_eq!(class.class_index, 0);
        assert_eq!(class.descriptor, "Lx;");
        assert_eq!(class.direct_method_count, 1);
        assert_eq!(class.methods[0].method_index, 0);
        assert_eq!(class.methods[0].name, "foo");
        assert_eq!(class.methods[0].prototype, "()Lx;");
        assert_eq!(class.methods[0].code_offset, 0);
    }

    fn sample_with_code(opcode: u16) -> (Vec<u8>, usize) {
        let mut b = sample(0);
        let strings = 0x70usize;
        let data = 0xb4usize;
        let string_data = data + 7;

        // Expand the method's code_off ULEB128 from one byte to two bytes.
        b[data + 6] = 0xc8; // low seven bits of aligned code offset 0xc8
        b.insert(data + 7, 0x01);
        let shifted_string_data = string_data + 1;
        b[strings..strings + 4].copy_from_slice(&(shifted_string_data as u32).to_le_bytes());
        b[strings + 4..strings + 8]
            .copy_from_slice(&((shifted_string_data + 5) as u32).to_le_bytes());

        while b.len() < 0xc8 { b.push(0); }
        let code_offset = b.len();
        let mut header = [0u8; 16];
        header[0..2].copy_from_slice(&1u16.to_le_bytes()); // registers_size
        header[2..4].copy_from_slice(&1u16.to_le_bytes()); // one incoming receiver word
        header[12..16].copy_from_slice(&1u32.to_le_bytes()); // one code unit
        b.extend_from_slice(&header);
        b.extend_from_slice(&opcode.to_le_bytes());
        let file_size = b.len() as u32;
        b[0x20..0x24].copy_from_slice(&file_size.to_le_bytes());
        (b, code_offset)
    }

    #[test]
    fn validates_instruction_streams_for_defined_methods() {
        let (bytes, _) = sample_with_code(0x000e); // return-void
        let inventory = inspect_classes(&bytes).unwrap();
        assert_eq!(inventory.classes[0].methods[0].code_offset, 0xc8);
    }

    #[test]
    fn rejects_unsupported_opcode_in_defined_method() {
        let (bytes, _) = sample_with_code(0x0073);
        assert_eq!(inspect_classes(&bytes), Err(ClassReadError::InvalidInstructions));
    }

    #[test]
    fn rejects_incoming_register_count_mismatch() {
        let (mut bytes, code_offset) = sample_with_code(0x000e);
        bytes[code_offset + 2..code_offset + 4].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(inspect_classes(&bytes), Err(ClassReadError::InvalidRegisterLayout));
    }

    #[test]
    fn accepts_static_method_without_receiver_word() {
        let (mut bytes, code_offset) = sample_with_code(0x000e);
        bytes[0xb4 + 5] = 0x08; // ACC_STATIC
        bytes[code_offset + 2..code_offset + 4].copy_from_slice(&0u16.to_le_bytes());
        let inventory = inspect_classes(&bytes).unwrap();
        assert_eq!(inventory.classes[0].methods[0].access_flags, 0x08);
    }

    #[test]
    fn rejects_missing_wide_parameter_words() {
        let mut bytes = sample(0);
        // The minimal fixture's prototype is ()Lx;; this test exercises the parser directly.
        assert_eq!(parameter_word_count("(JD)V"), Ok(4));
        assert_eq!(parameter_word_count("([JLjava/lang/Object;)V"), Ok(2));
        assert_eq!(parameter_word_count("(Q)V"), Err(ClassReadError::InvalidRegisterLayout));
        let _ = &mut bytes;
    }

    #[test]
    fn rejects_a_method_owned_by_another_class() {
        assert_eq!(inspect_classes(&sample(1)), Err(ClassReadError::InvalidIndex));
    }

    #[test]
    fn rejects_invalid_declared_file_size() {
        let mut b = sample(0);
        b[0x20..0x24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(inspect_classes(&b), Err(ClassReadError::InvalidFileSize));
    }
}
