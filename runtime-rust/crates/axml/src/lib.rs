//! Bounds-checked Android binary XML (AXML) parser.
//!
//! Parses Android resource string pools and the XML node structures needed to
//! identify an APK package and its launcher Activity. Every offset and length
//! is checked against its containing chunk before it is dereferenced.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxmlError {
    Truncated,
    InvalidMagic,
    InvalidFileSize,
    InvalidChunk,
    InvalidStringPool,
    InvalidStringPoolHeader,
    InvalidStringPoolFlags(u32),
    InvalidStringPoolOffsets,
    InvalidStringOffset { index: usize, offset: u32 },
    InvalidStringLength { index: usize },
    InvalidStringTerminator { index: usize },
    InvalidStringEncoding { index: usize },
    InvalidStylePool,
    InvalidStringIndex,
    UnsupportedStringEncoding,
    MalformedXmlNesting,
    MissingXmlRoot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringPool {
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDocument {
    pub file_size: u32,
    pub chunk_count: usize,
    pub string_pool: Option<StringPool>,
}

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const UTF8_FLAG: u32 = 0x0000_0100;
const SORTED_FLAG: u32 = 0x0000_0001;
const MAX_STRING_COUNT: usize = 1_000_000;

fn u16le(bytes: &[u8], offset: usize) -> Result<u16, AxmlError> {
    let end = offset.checked_add(2).ok_or(AxmlError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(AxmlError::Truncated)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32le(bytes: &[u8], offset: usize) -> Result<u32, AxmlError> {
    let end = offset.checked_add(4).ok_or(AxmlError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(AxmlError::Truncated)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn checked_end(start: usize, length: usize, limit: usize) -> Result<usize, AxmlError> {
    start.checked_add(length)
        .filter(|end| *end <= limit)
        .ok_or(AxmlError::InvalidStringPoolOffsets)
}

/// Android's variable-length UTF-8 string length (one or two bytes).
fn len8(bytes: &[u8], cursor: &mut usize, limit: usize) -> Result<usize, AxmlError> {
    let first = *bytes.get(*cursor).filter(|_| *cursor < limit)
        .ok_or(AxmlError::Truncated)? as usize;
    *cursor += 1;
    if first & 0x80 == 0 {
        return Ok(first);
    }
    let second = *bytes.get(*cursor).filter(|_| *cursor < limit)
        .ok_or(AxmlError::Truncated)? as usize;
    *cursor += 1;
    Ok(((first & 0x7f) << 8) | second)
}

/// Android's variable-length UTF-16 code-unit count (one or two little-endian words).
fn len16(bytes: &[u8], cursor: &mut usize, limit: usize) -> Result<usize, AxmlError> {
    let first_end = cursor.checked_add(2).ok_or(AxmlError::Truncated)?;
    if first_end > limit {
        return Err(AxmlError::Truncated);
    }
    let first = u16le(bytes, *cursor)? as usize;
    *cursor = first_end;
    if first & 0x8000 == 0 {
        return Ok(first);
    }
    let second_end = cursor.checked_add(2).ok_or(AxmlError::Truncated)?;
    if second_end > limit {
        return Err(AxmlError::Truncated);
    }
    let second = u16le(bytes, *cursor)? as usize;
    *cursor = second_end;
    Ok(((first & 0x7fff) << 16) | second)
}

fn parse_pool(bytes: &[u8], start: usize, chunk_size: usize) -> Result<StringPool, AxmlError> {
    let chunk_end = checked_end(start, chunk_size, bytes.len())
        .map_err(|_| AxmlError::InvalidStringPoolHeader)?;
    if chunk_size < 28 {
        return Err(AxmlError::InvalidStringPoolHeader);
    }

    let header_size = u16le(bytes, start + 2)? as usize;
    let string_count = u32le(bytes, start + 8)? as usize;
    let style_count = u32le(bytes, start + 12)? as usize;
    let flags = u32le(bytes, start + 16)?;
    let strings_start = u32le(bytes, start + 20)? as usize;
    let styles_start = u32le(bytes, start + 24)? as usize;

    if header_size < 28 || header_size > chunk_size
        || string_count > MAX_STRING_COUNT || style_count > MAX_STRING_COUNT
    {
        return Err(AxmlError::InvalidStringPoolHeader);
    }
    if flags & !(UTF8_FLAG | SORTED_FLAG) != 0 {
        return Err(AxmlError::InvalidStringPoolFlags(flags));
    }

    let string_offsets_bytes = string_count.checked_mul(4)
        .ok_or(AxmlError::InvalidStringPoolOffsets)?;
    let style_offsets_bytes = style_count.checked_mul(4)
        .ok_or(AxmlError::InvalidStringPoolOffsets)?;
    let string_offsets_start = start.checked_add(header_size)
        .ok_or(AxmlError::InvalidStringPoolOffsets)?;
    let string_offsets_end = checked_end(string_offsets_start, string_offsets_bytes, chunk_end)?;
    let style_offsets_end = checked_end(string_offsets_end, style_offsets_bytes, chunk_end)?;

    let string_data_start = if string_count == 0 {
        if strings_start != 0 && strings_start < header_size {
            return Err(AxmlError::InvalidStringPoolOffsets);
        }
        if strings_start == 0 { chunk_end } else {
            start.checked_add(strings_start).filter(|v| *v <= chunk_end)
                .ok_or(AxmlError::InvalidStringPoolOffsets)?
        }
    } else {
        if strings_start < header_size + string_offsets_bytes + style_offsets_bytes {
            return Err(AxmlError::InvalidStringPoolOffsets);
        }
        start.checked_add(strings_start).filter(|v| *v < chunk_end)
            .ok_or(AxmlError::InvalidStringPoolOffsets)?
    };

    let style_data_start = if style_count == 0 {
        chunk_end
    } else {
        if styles_start == 0 || styles_start < strings_start {
            return Err(AxmlError::InvalidStylePool);
        }
        start.checked_add(styles_start).filter(|v| *v <= chunk_end)
            .ok_or(AxmlError::InvalidStylePool)?
    };
    if style_count > 0 && style_data_start < string_data_start {
        return Err(AxmlError::InvalidStylePool);
    }

    let utf8 = flags & UTF8_FLAG != 0;
    let mut strings = Vec::with_capacity(string_count);

    for index in 0..string_count {
        let offset_position = string_offsets_start + index * 4;
        let relative = u32le(bytes, offset_position)?;
        let relative = usize::try_from(relative)
            .map_err(|_| AxmlError::InvalidStringOffset { index, offset: relative })?;
        let data_limit = if style_count > 0 { style_data_start } else { chunk_end };
        let mut cursor = string_data_start.checked_add(relative)
            .filter(|value| *value < data_limit)
            .ok_or(AxmlError::InvalidStringOffset {
                index,
                offset: relative as u32,
            })?;

        if utf8 {
            let utf16_length = len8(bytes, &mut cursor, data_limit)
                .map_err(|_| AxmlError::InvalidStringLength { index })?;
            let byte_length = len8(bytes, &mut cursor, data_limit)
                .map_err(|_| AxmlError::InvalidStringLength { index })?;
            let data_end = cursor.checked_add(byte_length)
                .filter(|end| *end < data_limit)
                .ok_or(AxmlError::InvalidStringLength { index })?;
            if bytes.get(data_end) != Some(&0) {
                return Err(AxmlError::InvalidStringTerminator { index });
            }
            let raw = &bytes[cursor..data_end];
            let value = std::str::from_utf8(raw)
                .map_err(|_| AxmlError::InvalidStringEncoding { index })?;
            if value.encode_utf16().count() != utf16_length {
                return Err(AxmlError::InvalidStringLength { index });
            }
            strings.push(value.to_owned());
        } else {
            let units = len16(bytes, &mut cursor, data_limit)
                .map_err(|_| AxmlError::InvalidStringLength { index })?;
            let byte_length = units.checked_mul(2)
                .ok_or(AxmlError::InvalidStringLength { index })?;
            let data_end = cursor.checked_add(byte_length)
                .filter(|end| end.checked_add(2).map(|v| v <= data_limit).unwrap_or(false))
                .ok_or(AxmlError::InvalidStringLength { index })?;
            let terminator = u16le(bytes, data_end)
                .map_err(|_| AxmlError::InvalidStringTerminator { index })?;
            if terminator != 0 {
                return Err(AxmlError::InvalidStringTerminator { index });
            }
            let mut words = Vec::with_capacity(units);
            for unit in 0..units {
                words.push(u16le(bytes, cursor + unit * 2)
                    .map_err(|_| AxmlError::InvalidStringLength { index })?);
            }
            let value = String::from_utf16(&words)
                .map_err(|_| AxmlError::InvalidStringEncoding { index })?;
            strings.push(value);
        }
    }

    // Validate style offsets even though style span data is not interpreted yet.
    for index in 0..style_count {
        let position = string_offsets_end + index * 4;
        let relative = u32le(bytes, position)?;
        if relative != u32::MAX {
            let relative = relative as usize;
            if relative >= chunk_end.saturating_sub(style_data_start) {
                return Err(AxmlError::InvalidStylePool);
            }
        }
    }

    let _ = style_offsets_end;
    Ok(StringPool { strings })
}

/// Walks an Android binary XML document, validating chunk bounds and decoding its string pool.
pub fn inspect_binary_xml(bytes: &[u8]) -> Result<XmlDocument, AxmlError> {
    if bytes.len() < 8 {
        return Err(AxmlError::Truncated);
    }
    if u16le(bytes, 0)? != RES_XML_TYPE || u16le(bytes, 2)? != 8 {
        return Err(AxmlError::InvalidMagic);
    }

    let file_size = u32le(bytes, 4)?;
    let file_end = usize::try_from(file_size).map_err(|_| AxmlError::InvalidFileSize)?;
    if file_end < 8 || file_end > bytes.len() {
        return Err(AxmlError::InvalidFileSize);
    }

    let mut cursor = 8usize;
    let mut chunk_count = 0usize;
    let mut string_pool = None;

    while cursor < file_end {
        if file_end - cursor < 8 {
            return Err(AxmlError::Truncated);
        }
        let chunk_type = u16le(bytes, cursor)?;
        let header_size = u16le(bytes, cursor + 2)? as usize;
        let chunk_size = usize::try_from(u32le(bytes, cursor + 4)?)
            .map_err(|_| AxmlError::InvalidChunk)?;
        if header_size < 8 || chunk_size < header_size
            || cursor.checked_add(chunk_size).filter(|end| *end <= file_end).is_none()
        {
            return Err(AxmlError::InvalidChunk);
        }
        if chunk_type == RES_STRING_POOL_TYPE {
            if string_pool.is_some() {
                return Err(AxmlError::InvalidStringPool);
            }
            string_pool = Some(parse_pool(bytes, cursor, chunk_size)?);
        }
        cursor += chunk_size;
        chunk_count += 1;
    }

    if cursor != file_end {
        return Err(AxmlError::InvalidChunk);
    }
    Ok(XmlDocument { file_size, chunk_count, string_pool })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestComponent {
    pub kind: String,
    pub name: String,
    pub exported: Option<bool>,
    pub has_launcher_intent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestMetadata {
    pub package_name: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<u32>,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
    pub permissions: Vec<String>,
    pub application_name: Option<String>,
    pub components: Vec<ManifestComponent>,
    pub launcher_activity: Option<String>,
}

fn string_at(pool: &StringPool, index: u32) -> Result<Option<String>, AxmlError> {
    if index == u32::MAX {
        return Ok(None);
    }
    pool.strings.get(index as usize).cloned()
        .map(Some).ok_or(AxmlError::InvalidStringIndex)
}

fn typed_attr(bytes: &[u8], offset: usize, pool: &StringPool) -> Result<Option<String>, AxmlError> {
    let raw_value = u32le(bytes, offset + 8)?;
    if raw_value != u32::MAX {
        return string_at(pool, raw_value);
    }
    let data_type = *bytes.get(offset + 15).ok_or(AxmlError::Truncated)?;
    let data = u32le(bytes, offset + 16)?;
    match data_type {
        0x03 => string_at(pool, data),
        0x10 | 0x11 => Ok(Some(data.to_string())),
        0x12 => Ok(Some((data != 0).to_string())),
        _ => Ok(None),
    }
}

/// Extracts package identity, SDK levels, permissions, application and launchable components.
pub fn parse_manifest(bytes: &[u8]) -> Result<ManifestMetadata, AxmlError> {
    let document = inspect_binary_xml(bytes)?;
    let pool = document.string_pool.ok_or(AxmlError::InvalidStringPool)?;
    let file_end = document.file_size as usize;
    let mut cursor = 8usize;
    let mut output = ManifestMetadata {
        package_name: None,
        version_name: None,
        version_code: None,
        min_sdk: None,
        target_sdk: None,
        permissions: Vec::new(),
        application_name: None,
        components: Vec::new(),
        launcher_activity: None,
    };
    let mut element_stack: Vec<String> = Vec::new();
    let mut component_stack: Vec<Option<usize>> = Vec::new();
    let mut intent_filters: Vec<(usize, bool, bool)> = Vec::new();
    let mut saw_root = false;

    while cursor < file_end {
        let kind = u16le(bytes, cursor)?;
        let header_size = u16le(bytes, cursor + 2)? as usize;
        let size = u32le(bytes, cursor + 4)? as usize;
        let chunk_end = cursor.checked_add(size)
            .filter(|end| *end <= file_end).ok_or(AxmlError::InvalidChunk)?;

        if kind == 0x0102 { // RES_XML_START_ELEMENT_TYPE
            if header_size < 16 || size < 36 {
                return Err(AxmlError::InvalidChunk);
            }
            let name_index = u32le(bytes, cursor + 20)?;
            let name = string_at(&pool, name_index)?.ok_or(AxmlError::InvalidStringIndex)?;
            let attribute_start = u16le(bytes, cursor + 24)? as usize;
            let attribute_size = u16le(bytes, cursor + 26)? as usize;
            let attribute_count = u16le(bytes, cursor + 28)? as usize;
            if attribute_size < 20 || attribute_start < 20 {
                return Err(AxmlError::InvalidChunk);
            }
            let extension_start = cursor.checked_add(16).ok_or(AxmlError::InvalidChunk)?;
            let attributes_start = extension_start.checked_add(attribute_start)
                .filter(|value| *value <= chunk_end).ok_or(AxmlError::InvalidChunk)?;
            let attributes_bytes = attribute_count.checked_mul(attribute_size)
                .ok_or(AxmlError::InvalidChunk)?;
            let attributes_end = attributes_start.checked_add(attributes_bytes)
                .filter(|end| *end <= chunk_end).ok_or(AxmlError::InvalidChunk)?;

            let mut attributes = std::collections::BTreeMap::<String, String>::new();
            for index in 0..attribute_count {
                let attribute = attributes_start + index * attribute_size;
                if attribute + 20 > attributes_end {
                    return Err(AxmlError::InvalidChunk);
                }
                let attribute_name = string_at(&pool, u32le(bytes, attribute + 4)?)?
                    .ok_or(AxmlError::InvalidStringIndex)?;
                if let Some(value) = typed_attr(bytes, attribute, &pool)? {
                    attributes.insert(attribute_name, value);
                }
            }

            let mut component_index = None;
            match name.as_str() {
                "manifest" => {
                    if saw_root {
                        return Err(AxmlError::MalformedXmlNesting);
                    }
                    saw_root = true;
                    output.package_name = attributes.get("package").cloned();
                    output.version_name = attributes.get("versionName").cloned();
                    output.version_code = attributes.get("versionCode").and_then(|v| v.parse().ok());
                }
                "uses-sdk" => {
                    output.min_sdk = attributes.get("minSdkVersion").and_then(|v| v.parse().ok());
                    output.target_sdk = attributes.get("targetSdkVersion").and_then(|v| v.parse().ok());
                }
                "uses-permission" | "uses-permission-sdk-23" | "uses-permission-sdk-m" => {
                    if let Some(value) = attributes.get("name") {
                        if !output.permissions.contains(value) {
                            output.permissions.push(value.clone());
                        }
                    }
                }
                "application" => output.application_name = attributes.get("name").cloned(),
                "intent-filter" => {
                    if let Some(Some(index)) = component_stack.last() {
                        intent_filters.push((*index, false, false));
                    }
                }
                "activity" | "activity-alias" | "service" | "receiver" | "provider" => {
                    if let Some(component_name) = attributes.get("name") {
                        let index = output.components.len();
                        output.components.push(ManifestComponent {
                            kind: name.clone(),
                            name: component_name.clone(),
                            exported: attributes.get("exported").map(|value| value == "true"),
                            has_launcher_intent: false,
                        });
                        component_index = Some(index);
                    }
                }
                "action" if attributes.get("name").map(String::as_str)
                    == Some("android.intent.action.MAIN") =>
                {
                    if let Some(filter) = intent_filters.last_mut() {
                        filter.1 = true;
                    }
                }
                "category" if attributes.get("name").map(String::as_str)
                    == Some("android.intent.category.LAUNCHER") =>
                {
                    if let Some(filter) = intent_filters.last_mut() {
                        filter.2 = true;
                    }
                }
                _ => {}
            }

            if component_index.is_none() {
                component_index = component_stack.last().copied().flatten();
            }
            element_stack.push(name);
            component_stack.push(component_index);
        } else if kind == 0x0103 { // RES_XML_END_ELEMENT_TYPE
            if header_size < 16 || size < 24 {
                return Err(AxmlError::InvalidChunk);
            }
            let name = string_at(&pool, u32le(bytes, cursor + 20)?)?
                .ok_or(AxmlError::InvalidStringIndex)?;
            if element_stack.pop().as_deref() != Some(name.as_str()) {
                return Err(AxmlError::MalformedXmlNesting);
            }
            component_stack.pop();
            if name == "intent-filter" {
                if let Some((index, has_main, has_launcher)) = intent_filters.pop() {
                    if has_main && has_launcher {
                        if let Some(component) = output.components.get_mut(index) {
                            component.has_launcher_intent = true;
                        }
                    }
                }
            }
        }
        cursor = chunk_end;
    }

    if !saw_root {
        return Err(AxmlError::MissingXmlRoot);
    }
    if !element_stack.is_empty() || !intent_filters.is_empty() {
        return Err(AxmlError::MalformedXmlNesting);
    }
    output.launcher_activity = output.components.iter()
        .find(|component| component.kind == "activity" && component.has_launcher_intent)
        .map(|component| component.name.clone());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn encode_len8(out: &mut Vec<u8>, length: usize) {
        if length > 0x7f {
            out.push(((length >> 8) as u8) | 0x80);
            out.push(length as u8);
        } else {
            out.push(length as u8);
        }
    }

    fn string_pool(strings: &[&str], utf8: bool) -> Vec<u8> {
        let mut offsets = Vec::new();
        let mut data = Vec::new();
        for value in strings {
            offsets.push(data.len() as u32);
            if utf8 {
                let utf16_len = value.encode_utf16().count();
                encode_len8(&mut data, utf16_len);
                encode_len8(&mut data, value.len());
                data.extend_from_slice(value.as_bytes());
                data.push(0);
            } else {
                let units: Vec<u16> = value.encode_utf16().collect();
                push_u16(&mut data, units.len() as u16);
                for unit in units {
                    push_u16(&mut data, unit);
                }
                push_u16(&mut data, 0);
            }
        }
        let header_size = 28usize;
        let strings_start = header_size + offsets.len() * 4;
        let chunk_size = strings_start + data.len();
        let mut out = Vec::with_capacity(chunk_size);
        push_u16(&mut out, RES_STRING_POOL_TYPE);
        push_u16(&mut out, header_size as u16);
        push_u32(&mut out, chunk_size as u32);
        push_u32(&mut out, strings.len() as u32);
        push_u32(&mut out, 0);
        push_u32(&mut out, if utf8 { UTF8_FLAG } else { 0 });
        push_u32(&mut out, strings_start as u32);
        push_u32(&mut out, 0);
        for offset in offsets {
            push_u32(&mut out, offset);
        }
        out.extend_from_slice(&data);
        out
    }

    fn xml_document(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body_len: usize = chunks.iter().map(Vec::len).sum();
        let mut out = Vec::with_capacity(8 + body_len);
        push_u16(&mut out, RES_XML_TYPE);
        push_u16(&mut out, 8);
        push_u32(&mut out, (8 + body_len) as u32);
        for chunk in chunks {
            out.extend_from_slice(chunk);
        }
        out
    }

    fn element_chunk(kind: u16, name_index: u32) -> Vec<u8> {
        let size = if kind == 0x0102 { 36 } else { 24 };
        let mut out = Vec::with_capacity(size);
        push_u16(&mut out, kind);
        push_u16(&mut out, 16);
        push_u32(&mut out, size as u32);
        push_u32(&mut out, 1); // line number
        push_u32(&mut out, u32::MAX); // comment
        push_u32(&mut out, u32::MAX); // namespace
        push_u32(&mut out, name_index);
        if kind == 0x0102 {
            push_u16(&mut out, 20); // attributeStart, relative to attrExt
            push_u16(&mut out, 20);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
        }
        out
    }

    #[test]
    fn rejects_plain_xml_and_truncated_documents() {
        assert_eq!(inspect_binary_xml(b"<manifest/>"), Err(AxmlError::InvalidMagic));
        assert_eq!(inspect_binary_xml(&[3, 0, 8]), Err(AxmlError::Truncated));
    }

    #[test]
    fn decodes_utf8_string_pool_including_multibyte_and_surrogate_lengths() {
        let pool = string_pool(&["manifest", "café", "🚗"], true);
        let document = inspect_binary_xml(&xml_document(&[pool])).unwrap();
        assert_eq!(document.string_pool.unwrap().strings, ["manifest", "café", "🚗"]);
    }

    #[test]
    fn decodes_utf16_string_pool_including_surrogate_pairs() {
        let pool = string_pool(&["manifest", "🚗"], false);
        let document = inspect_binary_xml(&xml_document(&[pool])).unwrap();
        assert_eq!(document.string_pool.unwrap().strings, ["manifest", "🚗"]);
    }

    #[test]
    fn reports_precise_invalid_string_pool_conditions() {
        let mut pool = string_pool(&["manifest"], true);
        // First string offset points beyond the string data.
        pool[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            inspect_binary_xml(&xml_document(&[pool])),
            Err(AxmlError::InvalidStringOffset { index: 0, .. })
        ));

        let mut pool = string_pool(&["x"], true);
        let last = pool.len() - 1;
        pool[last] = 0x7f; // corrupt the required UTF-8 NUL terminator
        assert_eq!(
            inspect_binary_xml(&xml_document(&[pool])),
            Err(AxmlError::InvalidStringTerminator { index: 0 })
        );
    }

    #[test]
    fn rejects_invalid_utf8_and_inconsistent_utf16_lengths() {
        let mut pool = string_pool(&["x"], true);
        let data_start = 28 + 4;
        pool[data_start + 2] = 0xff;
        assert_eq!(
            inspect_binary_xml(&xml_document(&[pool])),
            Err(AxmlError::InvalidStringEncoding { index: 0 })
        );

        let mut pool = string_pool(&["x"], true);
        let data_start = 28 + 4;
        pool[data_start] = 2; // declared UTF-16 length, actual is one
        assert_eq!(
            inspect_binary_xml(&xml_document(&[pool])),
            Err(AxmlError::InvalidStringLength { index: 0 })
        );
    }

    #[test]
    fn parses_a_minimal_binary_manifest_and_rejects_bad_nesting() {
        let pool = string_pool(&["manifest", "application"], true);
        let start = element_chunk(0x0102, 0);
        let end = element_chunk(0x0103, 0);
        let parsed = parse_manifest(&xml_document(&[pool.clone(), start.clone(), end])).unwrap();
        assert_eq!(parsed.package_name, None);
        assert_eq!(parsed.components.len(), 0);

        let nested_start = element_chunk(0x0102, 1);
        let wrong_end = element_chunk(0x0103, 0);
        let final_end = element_chunk(0x0103, 1);
        let malformed = xml_document(&[pool, start, nested_start, wrong_end, final_end]);
        assert_eq!(parse_manifest(&malformed), Err(AxmlError::MalformedXmlNesting));
    }

    #[test]
    fn rejects_manifest_without_string_pool_or_root() {
        assert_eq!(
            parse_manifest(&xml_document(&[])),
            Err(AxmlError::InvalidStringPool)
        );
        let pool = string_pool(&[], true);
        assert_eq!(
            parse_manifest(&xml_document(&[pool])),
            Err(AxmlError::MissingXmlRoot)
        );
    }
    #[test]
    fn parses_real_pocket_ridge_lite_manifest_from_latest_game_artifact() {
        let bytes = include_bytes!("../tests/fixtures/pocket-ridge-manifest.axml");
        let document = inspect_binary_xml(bytes).expect("real APK AXML should parse");
        assert_eq!(document.file_size as usize, bytes.len());
        assert_eq!(document.chunk_count, 18);

        let manifest = parse_manifest(bytes).expect("real APK manifest should parse");
        assert_eq!(manifest.package_name.as_deref(), Some("com.stride.compatgame"));
        assert_eq!(manifest.version_name.as_deref(), Some("1.0"));
        assert_eq!(manifest.version_code, Some(1));
        assert_eq!(manifest.min_sdk, Some(23));
        assert_eq!(manifest.target_sdk, Some(28));
        assert_eq!(
            manifest.launcher_activity.as_deref(),
            Some("com.stride.compatgame.MainActivity")
        );
        assert!(manifest.components.iter().any(|component| {
            component.kind == "activity"
                && component.name == "com.stride.compatgame.MainActivity"
                && component.exported == Some(true)
                && component.has_launcher_intent
        }));
    }

}
