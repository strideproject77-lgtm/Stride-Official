//! Dalvik and ART runtime value representation, register handling, and conversions.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Null,
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Object(u32),
}

impl Value {
    pub fn as_int(&self) -> i32 {
        match *self {
            Value::Int(v) => v,
            Value::Long(v) => v as i32,
            Value::Float(v) => v as i32,
            Value::Double(v) => v as i32,
            Value::Object(v) => v as i32,
            Value::Null => 0,
        }
    }

    pub fn as_long(&self) -> i64 {
        match *self {
            Value::Long(v) => v,
            Value::Int(v) => v as i64,
            Value::Float(v) => v as i64,
            Value::Double(v) => v as i64,
            Value::Object(v) => v as i64,
            Value::Null => 0,
        }
    }

    pub fn as_object(&self) -> Option<u32> {
        match *self {
            Value::Object(id) => Some(id),
            Value::Null => None,
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null) || matches!(self, Value::Object(0))
    }
}

/// A Dalvik method frame register array.
/// Registers are standard 32-bit words. Wide values (long and double)
/// occupy pairs of adjacent registers: (vA, vA+1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterFile {
    slots: Vec<u32>,
}

impl RegisterFile {
    pub fn new(size: usize) -> Self {
        Self {
            slots: vec![0; size],
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn get_raw(&self, index: u16) -> Option<u32> {
        self.slots.get(index as usize).copied()
    }

    pub fn set_raw(&mut self, index: u16, val: u32) -> Result<(), u16> {
        let slot = self.slots.get_mut(index as usize).ok_or(index)?;
        *slot = val;
        Ok(())
    }

    // --- 32-bit integer reads and writes ---
    pub fn read_int(&self, index: u16) -> Result<i32, u16> {
        self.get_raw(index).map(|w| w as i32).ok_or(index)
    }

    pub fn write_int(&mut self, index: u16, value: i32) -> Result<(), u16> {
        self.set_raw(index, value as u32)
    }

    // --- 64-bit wide (long) reads and writes ---
    pub fn read_long(&self, index: u16) -> Result<i64, u16> {
        let low = self.get_raw(index).ok_or(index)? as u64;
        let high = self.get_raw(index.checked_add(1).ok_or(index)?).ok_or(index + 1)? as u64;
        Ok(((high << 32) | low) as i64)
    }

    pub fn write_long(&mut self, index: u16, value: i64) -> Result<(), u16> {
        let uval = value as u64;
        let low = (uval & 0xffff_ffff) as u32;
        let high = ((uval >> 32) & 0xffff_ffff) as u32;
        self.set_raw(index, low)?;
        self.set_raw(index.checked_add(1).ok_or(index)?, high)?;
        Ok(())
    }

    // --- 32-bit float reads and writes ---
    pub fn read_float(&self, index: u16) -> Result<f32, u16> {
        self.get_raw(index).map(f32::from_bits).ok_or(index)
    }

    pub fn write_float(&mut self, index: u16, value: f32) -> Result<(), u16> {
        self.set_raw(index, value.to_bits())
    }

    // --- 64-bit double reads and writes ---
    pub fn read_double(&self, index: u16) -> Result<f64, u16> {
        let raw = self.read_long(index)? as u64;
        Ok(f64::from_bits(raw))
    }

    pub fn write_double(&mut self, index: u16, value: f64) -> Result<(), u16> {
        self.write_long(index, value.to_bits() as i64)
    }

    // --- Object references (u32 handles) ---
    pub fn read_object(&self, index: u16) -> Result<u32, u16> {
        self.get_raw(index).ok_or(index)
    }

    pub fn write_object(&mut self, index: u16, object_id: u32) -> Result<(), u16> {
        self.set_raw(index, object_id)
    }

    /// Load incoming arguments into the high end of the callee frame registers.
    pub fn initialize_incoming(&mut self, incoming_words: &[i32], ins_size: usize) -> Result<(), ()> {
        if incoming_words.len() > self.slots.len() || incoming_words.len() != ins_size {
            return Err(());
        }
        let start = self.slots.len() - incoming_words.len();
        for (offset, &word) in incoming_words.iter().enumerate() {
            self.slots[start + offset] = word as u32;
        }
        Ok(())
    }

    /// Read raw register slice
    pub fn raw_slice(&self) -> &[u32] {
        &self.slots
    }
}

// --- Conversions ---

pub fn int_to_long(v: i32) -> i64 {
    v as i64
}

pub fn int_to_float(v: i32) -> f32 {
    v as f32
}

pub fn int_to_double(v: i32) -> f64 {
    v as f64
}

pub fn long_to_int(v: i64) -> i32 {
    v as i32
}

pub fn long_to_float(v: i64) -> f32 {
    v as f32
}

pub fn long_to_double(v: i64) -> f64 {
    v as f64
}

pub fn float_to_int(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else if v >= i32::MAX as f32 {
        i32::MAX
    } else if v <= i32::MIN as f32 {
        i32::MIN
    } else {
        v as i32
    }
}

pub fn float_to_long(v: f32) -> i64 {
    if v.is_nan() {
        0
    } else if v >= i64::MAX as f32 {
        i64::MAX
    } else if v <= i64::MIN as f32 {
        i64::MIN
    } else {
        v as i64
    }
}

pub fn float_to_double(v: f32) -> f64 {
    v as f64
}

pub fn double_to_int(v: f64) -> i32 {
    if v.is_nan() {
        0
    } else if v >= i32::MAX as f64 {
        i32::MAX
    } else if v <= i32::MIN as f64 {
        i32::MIN
    } else {
        v as i32
    }
}

pub fn double_to_long(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else if v >= i64::MAX as f64 {
        i64::MAX
    } else if v <= i64::MIN as f64 {
        i64::MIN
    } else {
        v as i64
    }
}

pub fn double_to_float(v: f64) -> f32 {
    v as f32
}

pub fn int_to_byte(v: i32) -> i32 {
    (v as i8) as i32
}

pub fn int_to_char(v: i32) -> i32 {
    (v as u16) as i32
}

pub fn int_to_short(v: i32) -> i32 {
    (v as i16) as i32
}

// --- Comparisons ---

pub fn cmp_long(a: i64, b: i64) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

pub fn cmpl_float(a: f32, b: f32) -> i32 {
    if a.is_nan() || b.is_nan() {
        -1
    } else if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

pub fn cmpg_float(a: f32, b: f32) -> i32 {
    if a.is_nan() || b.is_nan() {
        1
    } else if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

pub fn cmpl_double(a: f64, b: f64) -> i32 {
    if a.is_nan() || b.is_nan() {
        -1
    } else if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

pub fn cmpg_double(a: f64, b: f64) -> i32 {
    if a.is_nan() || b.is_nan() {
        1
    } else if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_file_32bit_and_64bit_roundtrip() {
        let mut rf = RegisterFile::new(4);
        rf.write_int(0, 123456).unwrap();
        assert_eq!(rf.read_int(0), Ok(123456));

        rf.write_long(1, -9876543210123_i64).unwrap();
        assert_eq!(rf.read_long(1), Ok(-9876543210123_i64));

        rf.write_float(3, 1.25).unwrap();
        assert_eq!(rf.read_float(3), Ok(1.25));
    }

    #[test]
    fn type_conversions_match_dalvik_semantics() {
        assert_eq!(int_to_long(42), 42_i64);
        assert_eq!(long_to_int(0x1_0000_0002_i64), 2);
        assert_eq!(int_to_byte(300), 44);
        assert_eq!(int_to_char(65), 65);
        assert_eq!(int_to_short(70000), 4464);
        assert_eq!(cmp_long(10, 20), -1);
        assert_eq!(cmp_long(20, 10), 1);
        assert_eq!(cmp_long(10, 10), 0);
    }
}
