//! Bounds-checked Android resources.arsc value resolver (simple scalar entries).
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct ResourceCatalog {
    strings: BTreeMap<u32, String>,
    colors: BTreeMap<u32, u32>,
    files: BTreeMap<u32, String>,
}

fn u16_at(bytes: &[u8], offset: usize, limit: usize) -> Result<u16, String> {
    let end = offset.checked_add(2).filter(|end| *end <= limit)
        .ok_or_else(|| "truncated resource u16".to_owned())?;
    let value = bytes.get(offset..end).ok_or_else(|| "resource u16 outside input".to_owned())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn u32_at(bytes: &[u8], offset: usize, limit: usize) -> Result<u32, String> {
    let end = offset.checked_add(4).filter(|end| *end <= limit)
        .ok_or_else(|| "truncated resource u32".to_owned())?;
    let value = bytes.get(offset..end).ok_or_else(|| "resource u32 outside input".to_owned())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_len8(bytes: &[u8], cursor: &mut usize, limit: usize) -> Result<usize, String> {
    let first = *bytes.get(*cursor).filter(|_| *cursor < limit)
        .ok_or_else(|| "truncated UTF-8 string length".to_owned())? as usize;
    *cursor += 1;
    if first & 0x80 == 0 {
        return Ok(first);
    }
    let second = *bytes.get(*cursor).filter(|_| *cursor < limit)
        .ok_or_else(|| "truncated UTF-8 string length".to_owned())? as usize;
    *cursor += 1;
    Ok(((first & 0x7f) << 8) | second)
}

fn read_len16(bytes: &[u8], cursor: &mut usize, limit: usize) -> Result<usize, String> {
    let first = u16_at(bytes, *cursor, limit)? as usize;
    *cursor += 2;
    if first & 0x8000 == 0 {
        return Ok(first);
    }
    let second = u16_at(bytes, *cursor, limit)? as usize;
    *cursor += 2;
    Ok(((first & 0x7fff) << 16) | second)
}

fn parse_string_pool(bytes: &[u8], base: usize, chunk_size: usize) -> Result<Vec<String>, String> {
    let chunk_end = base.checked_add(chunk_size).filter(|end| *end <= bytes.len())
        .ok_or_else(|| "resource string pool exceeds input".to_owned())?;
    let header_size = u16_at(bytes, base + 2, chunk_end)? as usize;
    if header_size < 28 || header_size > chunk_size {
        return Err("invalid resource string-pool header size".into());
    }
    let string_count = u32_at(bytes, base + 8, chunk_end)? as usize;
    let style_count = u32_at(bytes, base + 12, chunk_end)? as usize;
    let flags = u32_at(bytes, base + 16, chunk_end)?;
    let strings_start = u32_at(bytes, base + 20, chunk_end)? as usize;
    let styles_start = u32_at(bytes, base + 24, chunk_end)? as usize;
    if string_count > 1_000_000 || style_count > 1_000_000 {
        return Err("resource string pool exceeds safe count bound".into());
    }
    let offsets_bytes = string_count.checked_mul(4)
        .ok_or_else(|| "resource string offset table overflow".to_owned())?;
    let offsets_start = base.checked_add(header_size)
        .ok_or_else(|| "resource string offset table overflow".to_owned())?;
    let offsets_end = offsets_start.checked_add(offsets_bytes)
        .filter(|end| *end <= chunk_end)
        .ok_or_else(|| "resource string offset table outside chunk".to_owned())?;
    let string_data_start = base.checked_add(strings_start)
        .filter(|start| *start <= chunk_end)
        .ok_or_else(|| "resource string data offset outside chunk".to_owned())?;
    let string_data_end = if styles_start == 0 {
        chunk_end
    } else {
        base.checked_add(styles_start)
            .filter(|end| *end >= string_data_start && *end <= chunk_end)
            .ok_or_else(|| "resource style data offset outside chunk".to_owned())?
    };
    if string_data_start > string_data_end {
        return Err("invalid resource string data range".into());
    }

    let utf8 = flags & 0x100 != 0;
    let mut strings = Vec::with_capacity(string_count);
    for index in 0..string_count {
        let relative = u32_at(bytes, offsets_start + index * 4, offsets_end)? as usize;
        let mut cursor = string_data_start.checked_add(relative)
            .filter(|at| *at < string_data_end)
            .ok_or_else(|| format!("resource string {index} offset outside string data"))?;
        if utf8 {
            let utf16_length = read_len8(bytes, &mut cursor, string_data_end)?;
            let byte_length = read_len8(bytes, &mut cursor, string_data_end)?;
            let end = cursor.checked_add(byte_length)
                .filter(|end| *end < string_data_end)
                .ok_or_else(|| format!("resource string {index} exceeds string data"))?;
            if bytes.get(end) != Some(&0) {
                return Err(format!("resource string {index} has no UTF-8 terminator"));
            }
            let raw = bytes.get(cursor..end)
                .ok_or_else(|| format!("resource string {index} is truncated"))?;
            let value = std::str::from_utf8(raw)
                .map_err(|_| format!("resource string {index} contains invalid UTF-8"))?;
            if value.encode_utf16().count() != utf16_length {
                return Err(format!("resource string {index} UTF-16 length mismatch"));
            }
            strings.push(value.to_owned());
        } else {
            let units = read_len16(bytes, &mut cursor, string_data_end)?;
            let byte_length = units.checked_mul(2)
                .ok_or_else(|| format!("resource string {index} UTF-16 length overflow"))?;
            let end = cursor.checked_add(byte_length)
                .filter(|end| end.checked_add(2).is_some_and(|terminator_end| terminator_end <= string_data_end))
                .ok_or_else(|| format!("resource string {index} exceeds UTF-16 string data"))?;
            if u16_at(bytes, end, string_data_end)? != 0 {
                return Err(format!("resource string {index} has no UTF-16 terminator"));
            }
            let mut words = Vec::with_capacity(units);
            let mut at = cursor;
            while at < end {
                words.push(u16_at(bytes, at, end)?);
                at += 2;
            }
            let value = String::from_utf16(&words)
                .map_err(|_| format!("resource string {index} contains invalid UTF-16"))?;
            strings.push(value);
        }
    }
    Ok(strings)
}

impl ResourceCatalog {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 12 || u16_at(data, 0, data.len())? != 0x0002 {
            return Err("invalid resources.arsc table header".into());
        }
        let table_header_size = u16_at(data, 2, data.len())? as usize;
        let total = u32_at(data, 4, data.len())? as usize;
        if table_header_size < 12 || total > data.len() || total < table_header_size {
            return Err("invalid resources.arsc size".into());
        }

        let mut catalog = Self::default();
        let mut global_strings = Vec::<String>::new();
        let mut position = table_header_size;
        while position.checked_add(8).is_some_and(|end| end <= total) {
            let kind = u16_at(data, position, total)?;
            let header_size = u16_at(data, position + 2, total)? as usize;
            let chunk_size = u32_at(data, position + 4, total)? as usize;
            let chunk_end = position.checked_add(chunk_size)
                .filter(|end| chunk_size >= header_size && header_size >= 8 && *end <= total)
                .ok_or_else(|| "resource chunk outside table bounds".to_owned())?;

            if kind == 0x0001 {
                global_strings = parse_string_pool(data, position, chunk_size)?;
            } else if kind == 0x0200 {
                if header_size < 12 {
                    return Err("resource package header is truncated".into());
                }
                let package_id = u32_at(data, position + 8, chunk_end)?;
                let mut child = position + header_size;
                while child.checked_add(8).is_some_and(|end| end <= chunk_end) {
                    let child_kind = u16_at(data, child, chunk_end)?;
                    let child_header = u16_at(data, child + 2, chunk_end)? as usize;
                    let child_size = u32_at(data, child + 4, chunk_end)? as usize;
                    let child_end = child.checked_add(child_size)
                        .filter(|end| child_size >= child_header && child_header >= 8 && *end <= chunk_end)
                        .ok_or_else(|| "invalid package resource chunk".to_owned())?;

                    if child_kind == 0x0201 {
                        if child_header < 20 {
                            return Err("resource type chunk header is truncated".into());
                        }
                        let type_id = u32::from(*data.get(child + 8).ok_or("missing resource type id")?);
                        let entry_count = u32_at(data, child + 12, child_end)? as usize;
                        let entries_start = u32_at(data, child + 16, child_end)? as usize;
                        if entry_count > 1_000_000 {
                            return Err("resource entry count exceeds bound".into());
                        }
                        let offsets_start = child.checked_add(child_header)
                            .ok_or_else(|| "resource entry offsets overflow".to_owned())?;
                        let offsets_end = offsets_start.checked_add(entry_count.checked_mul(4)
                            .ok_or_else(|| "resource entry offsets overflow".to_owned())?)
                            .filter(|end| *end <= child_end)
                            .ok_or_else(|| "resource entry offsets outside type chunk".to_owned())?;

                        for index in 0..entry_count {
                            let offset = u32_at(data, offsets_start + index * 4, offsets_end)?;
                            if offset == u32::MAX { continue; }
                            let entry = child.checked_add(entries_start)
                                .and_then(|start| start.checked_add(offset as usize))
                                .filter(|at| at.checked_add(8).is_some_and(|end| end <= child_end));
                            let Some(entry) = entry else { continue; };
                            let entry_size = u16_at(data, entry, child_end)? as usize;
                            let entry_flags = u16_at(data, entry + 2, child_end)?;
                            if entry_flags & 1 != 0 || entry_size < 8 { continue; }
                            let value_at = entry.checked_add(entry_size)
                                .filter(|at| at.checked_add(8).is_some_and(|end| end <= child_end));
                            let Some(value_at) = value_at else { continue; };
                            let value_size = u16_at(data, value_at, child_end)?;
                            if value_size < 8 { continue; }
                            let data_type = *data.get(value_at + 3).ok_or("missing resource value type")?;
                            let value = u32_at(data, value_at + 4, child_end)?;
                            let resource_id = ((package_id & 0xff) << 24)
                                | ((type_id & 0xff) << 16) | (index as u32 & 0xffff);
                            match data_type {
                                0x03 => if let Some(string) = global_strings.get(value as usize) {
                                    if string.starts_with("res/") || string.starts_with("assets/") {
                                        catalog.files.insert(resource_id, string.clone());
                                    } else {
                                        catalog.strings.insert(resource_id, string.clone());
                                    }
                                },
                                0x1c..=0x1f => { catalog.colors.insert(resource_id, value); }
                                _ => {}
                            }
                        }
                    }
                    child = child_end;
                }
            }
            position = chunk_end;
        }
        if position != total {
            return Err("resources.arsc contains an incomplete trailing chunk".into());
        }
        Ok(catalog)
    }

    pub fn string(&self, id: u32) -> Option<&str> {
        self.strings.get(&id).map(String::as_str)
    }

    pub fn color(&self, id: u32) -> Option<u32> {
        self.colors.get(&id).copied()
    }

    pub fn file(&self, id: u32) -> Option<&str> {
        self.files.get(&id).map(String::as_str)
    }

    pub fn file_entries(&self) -> Vec<(u32, String)> {
        self.files.iter().map(|(id, path)| (*id, path.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_table() {
        assert!(ResourceCatalog::parse(&[2, 0, 12, 0, 12, 0, 0, 0]).is_err());
    }

    fn utf8_pool(value: &[u8], utf16_length: usize) -> Vec<u8> {
        let mut chunk = vec![0u8; 28];
        chunk[0..2].copy_from_slice(&1u16.to_le_bytes());
        chunk[2..4].copy_from_slice(&28u16.to_le_bytes());
        chunk[4..8].copy_from_slice(&((28 + 4 + 2 + 1 + value.len() + 1) as u32).to_le_bytes());
        chunk[8..12].copy_from_slice(&1u32.to_le_bytes());
        chunk[16..20].copy_from_slice(&0x100u32.to_le_bytes());
        chunk[20..24].copy_from_slice(&32u32.to_le_bytes());
        chunk.extend_from_slice(&0u32.to_le_bytes());
        let mut payload = Vec::new();
        payload.push(utf16_length as u8);
        payload.push(value.len() as u8);
        payload.extend_from_slice(value);
        payload.push(0);
        chunk.extend_from_slice(&payload);
        let size = chunk.len() as u32;
        chunk[4..8].copy_from_slice(&size.to_le_bytes());
        chunk
    }

    #[test]
    fn decodes_utf8_resource_pool_without_changing_unicode() {
        let pool = utf8_pool("Tap anywhere to blink".as_bytes(), "Tap anywhere to blink".encode_utf16().count());
        let decoded = parse_string_pool(&pool, 0, pool.len()).unwrap();
        assert_eq!(decoded, vec!["Tap anywhere to blink"]);
    }

    #[test]
    fn rejects_invalid_utf8_instead_of_silently_replacing_bytes() {
        let pool = utf8_pool(&[0xc3, 0x28], 2);
        assert!(parse_string_pool(&pool, 0, pool.len()).is_err());
    }

    #[test]
    fn rejects_unpaired_surrogates_in_utf16_resource_strings() {
        let mut pool = vec![0u8; 28];
        pool[0..2].copy_from_slice(&1u16.to_le_bytes());
        pool[2..4].copy_from_slice(&28u16.to_le_bytes());
        pool[8..12].copy_from_slice(&1u32.to_le_bytes());
        pool[20..24].copy_from_slice(&32u32.to_le_bytes());
        pool.extend_from_slice(&0u32.to_le_bytes());
        pool.extend_from_slice(&1u16.to_le_bytes());
        pool.extend_from_slice(&0xd800u16.to_le_bytes());
        pool.extend_from_slice(&0u16.to_le_bytes());
        let size = pool.len() as u32;
        pool[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(parse_string_pool(&pool, 0, pool.len()).is_err());
    }

}
