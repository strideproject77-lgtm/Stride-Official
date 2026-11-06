//! DEX code_item structural reader. It exposes bounded instruction bytes; it is not a
//! Dalvik interpreter and deliberately does not claim bytecode execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeItemError { Truncated, InvalidOffset, InstructionDataOutOfBounds, InvalidTryPadding, TryItemsOutOfBounds }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeItem<'a> {
    pub registers_size: u16,
    pub ins_size: u16,
    pub outs_size: u16,
    pub tries_size: u16,
    pub debug_info_off: u32,
    pub insns_size: u32,
    pub instructions: &'a [u8],
    pub tries_offset: Option<usize>,
}

fn u16le(b:&[u8], o:usize)->Result<u16,CodeItemError>{
    let s=b.get(o..o.checked_add(2).ok_or(CodeItemError::Truncated)?).ok_or(CodeItemError::Truncated)?;
    Ok(u16::from_le_bytes([s[0],s[1]]))
}
fn u32le(b:&[u8], o:usize)->Result<u32,CodeItemError>{
    let s=b.get(o..o.checked_add(4).ok_or(CodeItemError::Truncated)?).ok_or(CodeItemError::Truncated)?;
    Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))
}

/// Reads one code_item at a DEX file offset and validates its instruction and try-item ranges.
pub fn read_code_item(bytes:&[u8], offset:u32)->Result<CodeItem<'_>,CodeItemError>{
    let start=usize::try_from(offset).map_err(|_|CodeItemError::InvalidOffset)?;
    if start < 0x70 || start.checked_add(16).filter(|e|*e<=bytes.len()).is_none(){return Err(CodeItemError::InvalidOffset);}
    let registers_size=u16le(bytes,start)?;
    let ins_size=u16le(bytes,start+2)?;
    let outs_size=u16le(bytes,start+4)?;
    let tries_size=u16le(bytes,start+6)?;
    let debug_info_off=u32le(bytes,start+8)?;
    let insns_size=u32le(bytes,start+12)?;
    let insns_start=start+16;
    let insns_bytes=usize::try_from(insns_size).ok().and_then(|n|n.checked_mul(2)).ok_or(CodeItemError::InstructionDataOutOfBounds)?;
    let insns_end=insns_start.checked_add(insns_bytes).filter(|e|*e<=bytes.len()).ok_or(CodeItemError::InstructionDataOutOfBounds)?;
    let tries_offset=if tries_size==0 {None} else {
        let aligned=if insns_size%2==1 {insns_end.checked_add(2).ok_or(CodeItemError::TryItemsOutOfBounds)?} else {insns_end};
        if aligned>bytes.len(){return Err(CodeItemError::InvalidTryPadding);}
        if insns_size%2==1 && bytes[insns_end..aligned]!=[0,0]{return Err(CodeItemError::InvalidTryPadding);}
        let count=usize::from(tries_size);
        let end=aligned.checked_add(count.checked_mul(8).ok_or(CodeItemError::TryItemsOutOfBounds)?).filter(|e|*e<=bytes.len()).ok_or(CodeItemError::TryItemsOutOfBounds)?;
        let _=end;
        Some(aligned)
    };
    Ok(CodeItem{registers_size,ins_size,outs_size,tries_size,debug_info_off,insns_size,instructions:&bytes[insns_start..insns_end],tries_offset})
}

#[cfg(test)]
mod code_item_tests {
 use super::*;
 fn sample(units:u32,tries:u16)->Vec<u8>{
  let padding = if tries > 0 && units % 2 == 1 { 2 } else { 0 };
  let try_bytes = usize::from(tries) * 8;
  let mut b=vec![0u8;0x70+16+(units as usize)*2+padding+try_bytes];
  let o=0x70;b[o..o+2].copy_from_slice(&4u16.to_le_bytes());b[o+6..o+8].copy_from_slice(&tries.to_le_bytes());b[o+12..o+16].copy_from_slice(&units.to_le_bytes());b
 }
 #[test] fn reads_code_item(){let b=sample(2,0);let c=read_code_item(&b,0x70).unwrap();assert_eq!(c.registers_size,4);assert_eq!(c.insns_size,2);assert_eq!(c.instructions.len(),4);}
 #[test] fn rejects_truncated_instructions(){let mut b=sample(3,0);b.truncate(b.len()-2);assert_eq!(read_code_item(&b,0x70),Err(CodeItemError::InstructionDataOutOfBounds));}
 #[test] fn rejects_missing_try_items(){let b=sample(2,1);let mut b=b;b.truncate(b.len()-1);assert_eq!(read_code_item(&b,0x70),Err(CodeItemError::TryItemsOutOfBounds));}
}
