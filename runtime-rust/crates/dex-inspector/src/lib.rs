//! DEX code_item structural reader. It exposes bounded instruction bytes; it is not a
//! Dalvik interpreter and deliberately does not claim bytecode execution.

pub mod classes;
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


// Dalvik instruction stream decoding. This is structural decoding only: no verifier,
// interpreter, JIT, or application execution is provided by this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionKind { Opcode, PackedSwitchPayload, SparseSwitchPayload, ArrayDataPayload }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedInstruction { pub code_unit_offset: usize, pub opcode: u8, pub width: usize, pub kind: InstructionKind }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError { OddByteLength, TruncatedInstruction, InvalidOpcode(u8), InvalidPayload, InvalidBranchTarget, InvalidPayloadTarget, InvalidRegister }

fn unit_at(units:&[u16], at:usize)->Result<u16,DecodeError>{units.get(at).copied().ok_or(DecodeError::TruncatedInstruction)}
fn signed32(units:&[u16], at:usize)->Result<i32,DecodeError>{
 let lo=unit_at(units,at)? as u32; let hi=unit_at(units,at+1)? as u32;
 Ok(((hi<<16)|lo) as i32)
}
fn opcode_width(op:u8)->Option<usize>{
 Some(match op {
  0x00|0x01|0x04|0x07|0x0a..=0x12|0x1d|0x1e|0x21|0x27|0x28|
  0x7b..=0x8f|0xb0..=0xcf => 1,
  0x02|0x05|0x08|0x13|0x15|0x16|0x19|0x1a|0x1c|0x1f|0x20|0x22|0x23|
  0x29|0x2d..=0x31|0x32..=0x3d|0x44..=0x6d|0x90..=0xaf|
  0xd0..=0xe2|0xfe|0xff => 2,
  0x03|0x06|0x09|0x14|0x17|0x1b|0x24|0x25|0x26|0x2a|0x2b|0x2c|
  0x6e..=0x72|0x74..=0x78|0xfc|0xfd => 3,
  0x18 => 5,
  0xfa|0xfb => 4,
  _ => return None,
 })
}

/// Checks encoded register indices for the instruction formats handled here.
/// This is a deliberately partial operand check, not the Dalvik verifier.
pub fn validate_register_operands(
    bytes: &[u8],
    registers_size: u16,
) -> Result<(), DecodeError> {
    if bytes.len() % 2 != 0 { return Err(DecodeError::OddByteLength); }
    let units: Vec<u16> = bytes.chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])).collect();
    let decoded = decode_instructions(bytes)?;
    let limit = usize::from(registers_size);
    let check = |register: usize| {
        if register < limit { Ok(()) } else { Err(DecodeError::InvalidRegister) }
    };
    for instruction in decoded {
        if instruction.kind != InstructionKind::Opcode { continue; }
        let pc = instruction.code_unit_offset;
        let first = unit_at(&units, pc)?;
        let op = instruction.opcode;
        let a8 = usize::from((first >> 8) & 0xff);
        let a4 = usize::from((first >> 8) & 0x0f);
        let b4 = usize::from((first >> 12) & 0x0f);
        match op {
            // 12x: two four-bit register operands.
            0x01 | 0x04 | 0x07 | 0x7b..=0x8f | 0xb0..=0xcf => {
                check(a4)?; check(b4)?;
            }
            // 11x and 11n: one byte or four-bit register.
            0x0a..=0x0d | 0x0f..=0x11 | 0x1d | 0x1e | 0x27 => check(a8)?,
            0x12 => check(a4)?,
            // 22x: A is a byte, B is the next full code unit.
            0x02 | 0x05 | 0x08 => {
                check(a8)?; check(usize::from(unit_at(&units, pc + 1)?))?;
            }
            // 32x: two full code-unit register indices.
            0x03 | 0x06 | 0x09 => {
                check(usize::from(unit_at(&units, pc + 1)?))?;
                check(usize::from(unit_at(&units, pc + 2)?))?;
            }
            // 21* formats: one byte register in the first unit.
            0x13 | 0x15 | 0x16 | 0x19 | 0x1a | 0x1c | 0x1f |
            0x22 | 0x26 | 0x29 | 0x2a | 0x2b | 0x2c |
            0x38..=0x3d | 0xfe | 0xff => check(a8)?,
            // 22t/22c/22s formats: two four-bit registers.
            0x20 | 0x23 | 0x32..=0x37 | 0xd0..=0xd7 => {
                check(a4)?; check(b4)?;
            }
            // 23x formats: A is in first unit, B/C in second.
            0x2d..=0x31 | 0x44..=0x6d | 0x90..=0xaf => {
                let second = unit_at(&units, pc + 1)?;
                check(a8)?;
                check(usize::from(second & 0xff))?;
                check(usize::from(second >> 8))?;
            }
            // 22b formats: A byte in first unit, B byte in second.
            0xd8..=0xe2 => {
                check(a8)?;
                check(usize::from(unit_at(&units, pc + 1)? & 0xff))?;
            }
            // Other formats are decoded for width/targets, but operand checks are not
            // claimed until their encoding-specific register layouts are implemented.
            _ => {}
        }
    }
    Ok(())
}

/// Decodes every code unit in a DEX instruction stream, validating instruction widths,
/// switch/array payload extents, and relative branch destinations.
pub fn decode_instructions(bytes:&[u8])->Result<Vec<DecodedInstruction>,DecodeError>{
 if bytes.len()%2!=0{return Err(DecodeError::OddByteLength);}
 let units:Vec<u16>=bytes.chunks_exact(2).map(|c|u16::from_le_bytes([c[0],c[1]])).collect();
 let mut decoded=Vec::new(); let mut payloads=Vec::new(); let mut branches=Vec::new(); let mut pc=0usize;
 while pc<units.len(){
  let first=units[pc]; let op=(first&0xff) as u8;
  if op==0 && first>>8!=0 {
   let ident=first>>8; let width=match ident {
    1=>{let n=unit_at(&units,pc+1)? as usize;4usize.checked_add(n.checked_mul(2).ok_or(DecodeError::InvalidPayload)?).ok_or(DecodeError::InvalidPayload)?},
    2=>{let n=unit_at(&units,pc+1)? as usize;2usize.checked_add(n.checked_mul(4).ok_or(DecodeError::InvalidPayload)?).ok_or(DecodeError::InvalidPayload)?},
    3=>{let element=unit_at(&units,pc+1)? as usize;if !matches!(element,1|2|4|8){return Err(DecodeError::InvalidPayload);}let lo=unit_at(&units,pc+2)? as u32;let hi=unit_at(&units,pc+3)? as u32;let count=((hi<<16)|lo) as usize;let data=count.checked_mul(element).ok_or(DecodeError::InvalidPayload)?;4usize.checked_add(data.checked_add(1).ok_or(DecodeError::InvalidPayload)?/2).ok_or(DecodeError::InvalidPayload)?},
    _=>return Err(DecodeError::InvalidPayload)
   };
   if width>units.len()-pc{return Err(DecodeError::TruncatedInstruction);}
   let kind=match ident{1=>InstructionKind::PackedSwitchPayload,2=>InstructionKind::SparseSwitchPayload,_=>InstructionKind::ArrayDataPayload};
   payloads.push((pc,kind));decoded.push(DecodedInstruction{code_unit_offset:pc,opcode:0,width,kind});pc+=width;continue;
  }
  let width=opcode_width(op).ok_or(DecodeError::InvalidOpcode(op))?;
  if width>units.len()-pc{return Err(DecodeError::TruncatedInstruction);}
  if op==0x28 {let d=(first as u8 as i8) as isize;branches.push((pc,d));}
  else if op==0x29 {branches.push((pc,unit_at(&units,pc+1)? as i16 as isize));}
  else if op==0x2a {branches.push((pc,signed32(&units,pc+1)? as isize));}
  else if (0x32..=0x3d).contains(&op) {branches.push((pc,unit_at(&units,pc+1)? as i16 as isize));}
  else if matches!(op,0x26|0x2b|0x2c) {let delta=signed32(&units,pc+1)? as isize;let target=(pc as isize).checked_add(delta).ok_or(DecodeError::InvalidPayloadTarget)?;if target<0||target as usize>=units.len(){return Err(DecodeError::InvalidPayloadTarget);}payloads.push((target as usize,match op{0x2b=>InstructionKind::PackedSwitchPayload,0x2c=>InstructionKind::SparseSwitchPayload,_=>InstructionKind::ArrayDataPayload}));}
  decoded.push(DecodedInstruction{code_unit_offset:pc,opcode:op,width,kind:InstructionKind::Opcode});pc+=width;
 }
 for (source,delta) in branches {
  let target=(source as isize).checked_add(delta).ok_or(DecodeError::InvalidBranchTarget)?;
  if target<0||target as usize>=units.len(){return Err(DecodeError::InvalidBranchTarget);}
  let target_kind=decoded.iter().find(|instruction|instruction.code_unit_offset==target as usize).map(|instruction|instruction.kind);
  if target_kind!=Some(InstructionKind::Opcode){return Err(DecodeError::InvalidBranchTarget);}
 }
 for (target,expected) in payloads {
  let found=decoded.iter().find(|i|i.code_unit_offset==target).map(|i|i.kind);
  if found!=Some(expected){return Err(DecodeError::InvalidPayloadTarget);}
 }
 Ok(decoded)
}

#[cfg(test)]
mod decoder_tests {
 use super::*;
 fn words(v:&[u16])->Vec<u8>{v.iter().flat_map(|x|x.to_le_bytes()).collect()}
 #[test] fn decodes_mixed_widths(){let d=decode_instructions(&words(&[0x0012,0x0013,0x0001,0x000e])).unwrap();assert_eq!(d.iter().map(|x|x.width).collect::<Vec<_>>(),vec![1,2,1]);}
 #[test] fn rejects_unknown_opcode(){assert_eq!(decode_instructions(&words(&[0x0073])),Err(DecodeError::InvalidOpcode(0x73)));}
 #[test] fn rejects_truncated_instruction(){assert_eq!(decode_instructions(&words(&[0x0013])),Err(DecodeError::TruncatedInstruction));}
 #[test] fn rejects_branch_outside_method(){assert_eq!(decode_instructions(&words(&[0xff28,0x000e])),Err(DecodeError::InvalidBranchTarget));}
 #[test] fn rejects_branch_into_instruction_operand(){assert_eq!(decode_instructions(&words(&[0x0129,0x0001,0x000e])),Err(DecodeError::InvalidBranchTarget));}

 #[test] fn rejects_out_of_range_register_in_12x(){
  assert_eq!(validate_register_operands(&words(&[0x0012,0x000e]),1),Ok(()));
  assert_eq!(validate_register_operands(&words(&[0x1012,0x000e]),1),Err(DecodeError::InvalidRegister));
 }
 #[test] fn validates_registers_in_22x_and_23x(){
  assert_eq!(validate_register_operands(&words(&[0x0002,0x0001,0x000e]),2),Ok(()));
  assert_eq!(validate_register_operands(&words(&[0x0002,0x0002,0x000e]),2),Err(DecodeError::InvalidRegister));
  assert_eq!(validate_register_operands(&words(&[0x0044,0x0201,0x000e]),3),Ok(()));
  assert_eq!(validate_register_operands(&words(&[0x0044,0x0301,0x000e]),3),Err(DecodeError::InvalidRegister));
 }
 #[test] fn validates_packed_switch_payload(){let d=decode_instructions(&words(&[0x002b,3,0,0x0100,1,0,0,0,0])).unwrap();assert_eq!(d[1].kind,InstructionKind::PackedSwitchPayload);}
}
