//! Bounds-checked ELF program-header inspection for Android ARM binaries.
//! No mapping, relocation, symbol resolution, or execution is performed here.
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub enum ElfClass{Elf32,Elf64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub enum ElfArchitecture{Arm,Aarch64,Other(u16)}
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub enum ElfByteOrder{Little,Big}
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub struct ElfIdentity{pub class:ElfClass,pub architecture:ElfArchitecture}
#[derive(Debug,Clone,PartialEq,Eq)] pub struct LoadSegment{pub file_offset:u64,pub virtual_address:u64,pub file_size:u64,pub memory_size:u64,pub flags:u32,pub alignment:u64}
#[derive(Debug,Clone,PartialEq,Eq)] pub struct ElfProgramInfo{pub identity:ElfIdentity,pub byte_order:ElfByteOrder,pub entry:u64,pub segments:Vec<LoadSegment>,pub dynamic_segment:Option<(u64,u64)>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)] pub enum ElfError{Truncated,InvalidMagic,UnsupportedClass(u8),UnsupportedByteOrder(u8),UnsupportedVersion(u8),InvalidHeader,ProgramHeadersOutOfBounds,InvalidSegment}
fn u16v(b:&[u8],o:usize,e:ElfByteOrder)->Result<u16,ElfError>{let s=b.get(o..o.checked_add(2).ok_or(ElfError::Truncated)?).ok_or(ElfError::Truncated)?;Ok(match e{ElfByteOrder::Little=>u16::from_le_bytes([s[0],s[1]]),ElfByteOrder::Big=>u16::from_be_bytes([s[0],s[1]])})}
fn u32v(b:&[u8],o:usize,e:ElfByteOrder)->Result<u32,ElfError>{let s=b.get(o..o.checked_add(4).ok_or(ElfError::Truncated)?).ok_or(ElfError::Truncated)?;Ok(match e{ElfByteOrder::Little=>u32::from_le_bytes(s.try_into().unwrap()),ElfByteOrder::Big=>u32::from_be_bytes(s.try_into().unwrap())})}
fn u64v(b:&[u8],o:usize,e:ElfByteOrder)->Result<u64,ElfError>{let s=b.get(o..o.checked_add(8).ok_or(ElfError::Truncated)?).ok_or(ElfError::Truncated)?;Ok(match e{ElfByteOrder::Little=>u64::from_le_bytes(s.try_into().unwrap()),ElfByteOrder::Big=>u64::from_be_bytes(s.try_into().unwrap())})}
/// Parses ELF identification and PT_LOAD/PT_DYNAMIC program headers with strict file bounds.
pub fn inspect_elf(bytes:&[u8])->Result<ElfProgramInfo,ElfError>{
 if bytes.len()<52{return Err(ElfError::Truncated);}if &bytes[..4]!=b"\x7fELF"{return Err(ElfError::InvalidMagic);}
 let class=match bytes[4]{1=>ElfClass::Elf32,2=>ElfClass::Elf64,v=>return Err(ElfError::UnsupportedClass(v))};
 let order=match bytes[5]{1=>ElfByteOrder::Little,2=>ElfByteOrder::Big,v=>return Err(ElfError::UnsupportedByteOrder(v))};
 if bytes[6]!=1{return Err(ElfError::UnsupportedVersion(bytes[6]));}
 let machine=u16v(bytes,18,order)?;let architecture=match machine{40=>ElfArchitecture::Arm,183=>ElfArchitecture::Aarch64,v=>ElfArchitecture::Other(v)};
 let (entry,phoff,ehsize,phentsize,phnum)=match class{
  ElfClass::Elf32=>(u32v(bytes,24,order)? as u64,u32v(bytes,28,order)? as u64,u16v(bytes,40,order)?,u16v(bytes,42,order)?,u16v(bytes,44,order)?),
  ElfClass::Elf64=>(u64v(bytes,24,order)?,u64v(bytes,32,order)?,u16v(bytes,52,order)?,u16v(bytes,54,order)?,u16v(bytes,56,order)?)
 };
 let min_ent=if class==ElfClass::Elf32{32}else{56};
 if ehsize<(if class==ElfClass::Elf32{52}else{64})||phentsize<min_ent{return Err(ElfError::InvalidHeader);}
 let start=usize::try_from(phoff).map_err(|_|ElfError::ProgramHeadersOutOfBounds)?;
 let total=(phentsize as usize).checked_mul(phnum as usize).ok_or(ElfError::ProgramHeadersOutOfBounds)?;
 let end=start.checked_add(total).filter(|e|*e<=bytes.len()).ok_or(ElfError::ProgramHeadersOutOfBounds)?;
 let mut segments=Vec::new();let mut dynamic=None;
 for i in 0..phnum as usize{
  let p=start+i*phentsize as usize;
  let (kind,flags,off,va,filesz,memsz,align)=match class{
   ElfClass::Elf32=>(u32v(bytes,p,order)?,u32v(bytes,p+24,order)?,u32v(bytes,p+4,order)? as u64,u32v(bytes,p+8,order)? as u64,u32v(bytes,p+16,order)? as u64,u32v(bytes,p+20,order)? as u64,u32v(bytes,p+28,order)? as u64),
   ElfClass::Elf64=>(u32v(bytes,p,order)?,u32v(bytes,p+4,order)?,u64v(bytes,p+8,order)?,u64v(bytes,p+16,order)?,u64v(bytes,p+32,order)?,u64v(bytes,p+40,order)?,u64v(bytes,p+48,order)?)
  };
  if filesz>memsz||off.checked_add(filesz).filter(|v|*v<=bytes.len() as u64).is_none(){return Err(ElfError::InvalidSegment);}
  if kind==1{segments.push(LoadSegment{file_offset:off,virtual_address:va,file_size:filesz,memory_size:memsz,flags,alignment:align});}
  if kind==2{if dynamic.is_some(){return Err(ElfError::InvalidSegment);}dynamic=Some((off,filesz));}
 }
 let _=end;
 Ok(ElfProgramInfo{identity:ElfIdentity{class,architecture},byte_order:order,entry,segments,dynamic_segment:dynamic})
}
