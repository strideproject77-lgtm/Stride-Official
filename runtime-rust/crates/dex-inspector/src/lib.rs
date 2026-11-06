//! Bounds-checked DEX container inspection.
//! This validates structural metadata only; it does not execute APK code.
const HEADER_SIZE: usize = 0x70;
const ENDIAN_CONSTANT: u32 = 0x1234_5678;
const REVERSE_ENDIAN_CONSTANT: u32 = 0x7856_3412;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DexByteOrder { Little, Big }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DexTable { pub count: u32, pub offset: u32 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexHeader {
    pub version: [u8; 3], pub byte_order: DexByteOrder,
    pub file_size: u32, pub header_size: u32, pub map_offset: u32,
    pub string_ids: DexTable, pub type_ids: DexTable, pub proto_ids: DexTable,
    pub field_ids: DexTable, pub method_ids: DexTable, pub class_defs: DexTable,
    pub data_size: u32, pub data_offset: u32, pub map_item_count: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DexError {
    Truncated, InvalidMagic, UnsupportedVersion, InvalidHeaderSize(u32),
    InvalidEndianTag(u32), InvalidFileSize(u32),
    TableOutOfBounds { table: &'static str }, InvalidDataSection,
    InvalidMapOffset, InvalidMapList,
}

fn read_u32(bytes: &[u8], offset: usize, order: DexByteOrder) -> Result<u32, DexError> {
    let s = bytes.get(offset..offset + 4).ok_or(DexError::Truncated)?;
    let raw = [s[0], s[1], s[2], s[3]];
    Ok(match order { DexByteOrder::Little => u32::from_le_bytes(raw), DexByteOrder::Big => u32::from_be_bytes(raw) })
}

fn validate_table(t: DexTable, item_size: u32, file_size: u32, name: &'static str) -> Result<(), DexError> {
    if t.count == 0 { return Ok(()); }
    let size = t.count.checked_mul(item_size).ok_or(DexError::TableOutOfBounds { table: name })?;
    let end = t.offset.checked_add(size).ok_or(DexError::TableOutOfBounds { table: name })?;
    if t.offset < HEADER_SIZE as u32 || end > file_size { return Err(DexError::TableOutOfBounds { table: name }); }
    Ok(())
}

/// Parse a DEX 035/037–041 header and validate all fixed-size tables and map bounds.
pub fn inspect_dex(bytes: &[u8]) -> Result<DexHeader, DexError> {
    if bytes.len() < HEADER_SIZE { return Err(DexError::Truncated); }
    if &bytes[0..4] != b"dex\n" || bytes[7] != 0 { return Err(DexError::InvalidMagic); }
    let version = [bytes[4], bytes[5], bytes[6]];
    if !matches!(&version, b"035" | b"037" | b"038" | b"039" | b"040" | b"041") { return Err(DexError::UnsupportedVersion); }
    let endian_raw = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
    let order = match endian_raw {
        ENDIAN_CONSTANT => DexByteOrder::Little,
        REVERSE_ENDIAN_CONSTANT => DexByteOrder::Big,
        v => return Err(DexError::InvalidEndianTag(v)),
    };
    let file_size = read_u32(bytes, 32, order)?;
    let header_size = read_u32(bytes, 36, order)?;
    if header_size != HEADER_SIZE as u32 { return Err(DexError::InvalidHeaderSize(header_size)); }
    if file_size < HEADER_SIZE as u32 || file_size as usize > bytes.len() { return Err(DexError::InvalidFileSize(file_size)); }
    let table = |c, o| -> Result<DexTable, DexError> { Ok(DexTable { count: read_u32(bytes, c, order)?, offset: read_u32(bytes, o, order)? }) };
    let string_ids = table(56, 60)?;
    let type_ids = table(64, 68)?;
    let proto_ids = table(72, 76)?;
    let field_ids = table(80, 84)?;
    let method_ids = table(88, 92)?;
    let class_defs = table(96, 100)?;
    validate_table(string_ids, 4, file_size, "string_ids")?;
    validate_table(type_ids, 4, file_size, "type_ids")?;
    validate_table(proto_ids, 12, file_size, "proto_ids")?;
    validate_table(field_ids, 8, file_size, "field_ids")?;
    validate_table(method_ids, 8, file_size, "method_ids")?;
    validate_table(class_defs, 32, file_size, "class_defs")?;
    let data_size = read_u32(bytes, 104, order)?;
    let data_offset = read_u32(bytes, 108, order)?;
    if data_size == 0 {
        if data_offset != 0 { return Err(DexError::InvalidDataSection); }
    } else {
        let end = data_offset.checked_add(data_size).ok_or(DexError::InvalidDataSection)?;
        if data_offset < HEADER_SIZE as u32 || end > file_size { return Err(DexError::InvalidDataSection); }
    }
    let map_offset = read_u32(bytes, 52, order)?;
    let map_item_count = if map_offset == 0 { None } else {
        if map_offset < HEADER_SIZE as u32 || map_offset.checked_add(4).filter(|v| *v <= file_size).is_none() { return Err(DexError::InvalidMapOffset); }
        let count = read_u32(bytes, map_offset as usize, order)?;
        let size = count.checked_mul(12).ok_or(DexError::InvalidMapList)?;
        let end = map_offset.checked_add(4).and_then(|v| v.checked_add(size)).ok_or(DexError::InvalidMapList)?;
        if end > file_size { return Err(DexError::InvalidMapList); }
        Some(count)
    };
    Ok(DexHeader { version, byte_order: order, file_size, header_size, map_offset, string_ids, type_ids, proto_ids, field_ids, method_ids, class_defs, data_size, data_offset, map_item_count })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn minimal() -> Vec<u8> {
        let mut b = vec![0; HEADER_SIZE];
        b[0..8].copy_from_slice(b"dex\n035\0");
        b[32..36].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        b[36..40].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        b[40..44].copy_from_slice(&ENDIAN_CONSTANT.to_le_bytes());
        b
    }
    #[test] fn parses_minimal_header() {
        let h = inspect_dex(&minimal()).unwrap();
        assert_eq!(h.version, *b"035"); assert_eq!(h.byte_order, DexByteOrder::Little);
    }
    #[test] fn rejects_bad_magic_and_version() {
        let mut b = minimal(); b[0] = b'x'; assert_eq!(inspect_dex(&b), Err(DexError::InvalidMagic));
        let mut b = minimal(); b[4..7].copy_from_slice(b"999"); assert_eq!(inspect_dex(&b), Err(DexError::UnsupportedVersion));
    }
    #[test] fn rejects_out_of_bounds_table() {
        let mut b = minimal(); b[56..60].copy_from_slice(&1u32.to_le_bytes()); b[60..64].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
        assert_eq!(inspect_dex(&b), Err(DexError::TableOutOfBounds { table: "string_ids" }));
    }
    #[test] fn rejects_truncated_map_list() {
        let mut b = minimal(); b.resize(HEADER_SIZE + 4, 0);
        b[32..36].copy_from_slice(&((HEADER_SIZE + 4) as u32).to_le_bytes());
        b[52..56].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        b[HEADER_SIZE..].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(inspect_dex(&b), Err(DexError::InvalidMapList));
    }
    #[test] fn accepts_reverse_endian_header() {
        let mut b = minimal(); b[40..44].copy_from_slice(&REVERSE_ENDIAN_CONSTANT.to_le_bytes());
        b[32..36].copy_from_slice(&(HEADER_SIZE as u32).to_be_bytes());
        b[36..40].copy_from_slice(&(HEADER_SIZE as u32).to_be_bytes());
        assert_eq!(inspect_dex(&b).unwrap().byte_order, DexByteOrder::Big);
    }
}
