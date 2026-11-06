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


// ELF dynamic-table, dynamic-symbol and relocation inspection. All addresses are
// translated through file-backed PT_LOAD ranges; this does not map or execute code.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum RelocationKind { Rel, Rela, PltRel, PltRela }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct DynamicSymbol { pub index:u32, pub name:String, pub value:u64, pub size:u64, pub binding:u8, pub symbol_type:u8, pub section_index:u16 }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Relocation { pub kind:RelocationKind, pub offset:u64, pub symbol_index:u32, pub relocation_type:u32, pub addend:Option<i64> }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ElfDynamicInfo { pub symbols:Vec<DynamicSymbol>, pub relocations:Vec<Relocation>, pub dynamic_tags:Vec<(i64,u64)> }
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum DynamicError { MissingDynamicSegment, InvalidDynamicTable, MissingStringTable, MissingSymbolTable, InvalidStringTable, InvalidSymbolTable, InvalidHashTable, InvalidRelocationTable, UnsupportedSymbolEntrySize }
fn vaddr_offset(info:&ElfProgramInfo,addr:u64,size:u64)->Result<usize,DynamicError>{
 for s in &info.segments {if addr>=s.virtual_address {let delta=addr-s.virtual_address;if delta.checked_add(size).map(|e|e<=s.file_size).unwrap_or(false){return usize::try_from(s.file_offset+delta).map_err(|_|DynamicError::InvalidDynamicTable);}}}
 Err(DynamicError::InvalidDynamicTable)
}
fn c_string(b:&[u8],start:usize,max:usize)->Result<String,DynamicError>{
 let end=start.checked_add(max).filter(|e|*e<=b.len()).ok_or(DynamicError::InvalidStringTable)?;
 let rel=b[start..end].iter().position(|x|*x==0).ok_or(DynamicError::InvalidStringTable)?;
 Ok(String::from_utf8_lossy(&b[start..start+rel]).into_owned())
}
/// Reads DT_* metadata, dynamic symbols (using DT_HASH or DT_GNU_HASH), and REL/RELA tables.
pub fn inspect_elf_dynamic(bytes:&[u8],info:&ElfProgramInfo)->Result<ElfDynamicInfo,DynamicError>{
 let (dyn_off,dyn_size)=info.dynamic_segment.ok_or(DynamicError::MissingDynamicSegment)?;
 let is64=info.identity.class==ElfClass::Elf64;let ent=if is64{16usize}else{8usize};
 if dyn_size%(ent as u64)!=0{return Err(DynamicError::InvalidDynamicTable);}
 let mut tags=Vec::new();let mut p=usize::try_from(dyn_off).map_err(|_|DynamicError::InvalidDynamicTable)?;
 let dend=p.checked_add(dyn_size as usize).filter(|e|*e<=bytes.len()).ok_or(DynamicError::InvalidDynamicTable)?;
 while p+ent<=dend {
  let (tag,val)=if is64{(u64v(bytes,p,info.byte_order).map_err(|_|DynamicError::InvalidDynamicTable)? as i64,u64v(bytes,p+8,info.byte_order).map_err(|_|DynamicError::InvalidDynamicTable)?)}else{(u32v(bytes,p,info.byte_order).map_err(|_|DynamicError::InvalidDynamicTable)? as i32 as i64,u32v(bytes,p+4,info.byte_order).map_err(|_|DynamicError::InvalidDynamicTable)? as u64)};
  p+=ent;if tag==0{break;}tags.push((tag,val));
 }
 let get=|tag:i64|tags.iter().find(|x|x.0==tag).map(|x|x.1);
 let str_addr=get(5).ok_or(DynamicError::MissingStringTable)?;let str_size=get(10).ok_or(DynamicError::MissingStringTable)?;
 let str_off=vaddr_offset(info,str_addr,str_size).map_err(|_|DynamicError::InvalidStringTable)?;
 let str_end=str_off.checked_add(str_size as usize).filter(|e|*e<=bytes.len()).ok_or(DynamicError::InvalidStringTable)?;
 let strings=&bytes[str_off..str_end];
 let sym_addr=get(6).ok_or(DynamicError::MissingSymbolTable)?;let sym_ent=get(11).unwrap_or(if is64{24}else{16}) as usize;
 if sym_ent<(if is64{24}else{16}){return Err(DynamicError::UnsupportedSymbolEntrySize);}
 let sym_count=if let Some(hash)=get(4){
  let o=vaddr_offset(info,hash,8).map_err(|_|DynamicError::InvalidHashTable)?;
  u32v(bytes,o+4,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)? as usize
 } else if let Some(hash)=get(0x6ffffef5){
  let o=vaddr_offset(info,hash,16).map_err(|_|DynamicError::InvalidHashTable)?;
  let buckets=u32v(bytes,o,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)? as usize;
  let symoff=u32v(bytes,o+4,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)? as usize;
  let bloom=u32v(bytes,o+8,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)? as usize;
  let word=if is64{8usize}else{4usize};
  let bucket_off=o.checked_add(16).and_then(|v|v.checked_add(bloom.checked_mul(word)?)).ok_or(DynamicError::InvalidHashTable)?;
  let chain_off=bucket_off.checked_add(buckets.checked_mul(4).ok_or(DynamicError::InvalidHashTable)?).ok_or(DynamicError::InvalidHashTable)?;
  let mut max=symoff;
  for i in 0..buckets {
   let first=u32v(bytes,bucket_off+i*4,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)? as usize;
   if first==0{continue;}if first<symoff{return Err(DynamicError::InvalidHashTable);}
   let mut idx=first;let mut steps=0usize;
   loop {let ci=idx-symoff;let co=chain_off.checked_add(ci*4).ok_or(DynamicError::InvalidHashTable)?;let word=u32v(bytes,co,info.byte_order).map_err(|_|DynamicError::InvalidHashTable)?;max=max.max(idx+1);steps+=1;if word&1!=0{break;}idx+=1;if steps>1_000_000{return Err(DynamicError::InvalidHashTable);}}
  }max
 } else {return Err(DynamicError::InvalidHashTable)};
 if sym_count>1_000_000{return Err(DynamicError::InvalidSymbolTable);}
 let sym_bytes=(sym_count as u64).checked_mul(sym_ent as u64).ok_or(DynamicError::InvalidSymbolTable)?;
 let sym_off=vaddr_offset(info,sym_addr,sym_bytes).map_err(|_|DynamicError::InvalidSymbolTable)?;
 let mut symbols=Vec::with_capacity(sym_count);
 for i in 0..sym_count {
  let q=sym_off+i*sym_ent;
  let (name,value,size,info_byte,shndx)=if is64{
   (u32v(bytes,q,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?,u64v(bytes,q+8,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?,u64v(bytes,q+16,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?,*bytes.get(q+4).ok_or(DynamicError::InvalidSymbolTable)?,u16v(bytes,q+6,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?)
  }else{
   (u32v(bytes,q,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?,u32v(bytes,q+4,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)? as u64,u32v(bytes,q+8,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)? as u64,*bytes.get(q+12).ok_or(DynamicError::InvalidSymbolTable)?,u16v(bytes,q+14,info.byte_order).map_err(|_|DynamicError::InvalidSymbolTable)?)
  };
  let name=if name==0{String::new()}else{let n=name as usize;if n>=strings.len(){return Err(DynamicError::InvalidStringTable);}c_string(strings,n,strings.len()-n)?};
  symbols.push(DynamicSymbol{index:i as u32,name,value,size,binding:info_byte>>4,symbol_type:info_byte&0xf,section_index:shndx});
 }
 let mut relocations=Vec::new();
 let mut parse_reloc=|addr:u64,size:u64,kind:RelocationKind|->Result<(),DynamicError>{
  if size==0{return Ok(());}let rela=matches!(kind,RelocationKind::Rela|RelocationKind::PltRela);
  let width=match (is64,rela){(true,true)=>24,(true,false)=>16,(false,true)=>12,(false,false)=>8};
  let min=width as u64;if size%min!=0{return Err(DynamicError::InvalidRelocationTable);}
  let off=vaddr_offset(info,addr,size).map_err(|_|DynamicError::InvalidRelocationTable)?;
  for i in 0..(size as usize/width){let q=off+i*width;
   let (r_offset,r_info,add)=if is64{(u64v(bytes,q,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)?,u64v(bytes,q+8,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)?,if rela{Some(u64v(bytes,q+16,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)? as i64)}else{None})}else{(u32v(bytes,q,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)? as u64,u32v(bytes,q+4,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)? as u64,if rela{Some(u32v(bytes,q+8,info.byte_order).map_err(|_|DynamicError::InvalidRelocationTable)? as i32 as i64)}else{None})};
   let (si,ty)=if is64{((r_info>>32) as u32,r_info as u32)}else{((r_info>>8) as u32,(r_info&0xff) as u32)};
   relocations.push(Relocation{kind,offset:r_offset,symbol_index:si,relocation_type:ty,addend:add});
  }Ok(())
 };
 if let (Some(a),Some(s))=(get(17),get(18)){parse_reloc(a,s,RelocationKind::Rel)?;}
 if let (Some(a),Some(s))=(get(7),get(8)){parse_reloc(a,s,RelocationKind::Rela)?;}
 if let (Some(a),Some(s))=(get(23),get(2)){let k=if get(20)==Some(7){RelocationKind::PltRela}else{RelocationKind::PltRel};parse_reloc(a,s,k)?;}
 Ok(ElfDynamicInfo{symbols,relocations,dynamic_tags:tags})
}

#[cfg(test)]
mod dynamic_tests {
 use super::*;
 #[test] fn rejects_missing_dynamic_segment(){let i=ElfProgramInfo{identity:ElfIdentity{class:ElfClass::Elf64,architecture:ElfArchitecture::Aarch64},byte_order:ElfByteOrder::Little,entry:0,segments:vec![],dynamic_segment:None};assert_eq!(inspect_elf_dynamic(&[],&i),Err(DynamicError::MissingDynamicSegment));}
}
