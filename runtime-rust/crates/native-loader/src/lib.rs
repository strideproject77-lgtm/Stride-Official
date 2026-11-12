//! Android ELF image layout planning for Stride's native loader.
//! Converts inspected ELF program headers into a checked 16 KiB page plan.
//! It does not map executable memory or call Android code.
use std::collections::BTreeMap;
use stride_elf_inspector::{ElfArchitecture, ElfProgramInfo};
pub const IOS_PAGE_SIZE:u64=16*1024;
#[derive(Debug,Clone,Copy,PartialEq,Eq,Default)]
pub struct PageProtection{pub readable:bool,pub writable:bool,pub executable:bool}
impl PageProtection{
 fn from_elf(flags:u32)->Self{Self{executable:flags&1!=0,writable:flags&2!=0,readable:flags&4!=0}}
 fn merge(&mut self,other:Self)->Result<(),LoadPlanError>{self.readable|=other.readable;self.writable|=other.writable;self.executable|=other.executable;if self.writable&&self.executable{return Err(LoadPlanError::WritableExecutablePage);}Ok(())}
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct SegmentCopy{pub file_offset:u64,pub image_offset:u64,pub file_size:u64,pub memory_size:u64}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct PlannedPage{pub image_offset:u64,pub protection:PageProtection}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct NativeImagePlan{pub load_bias:u64,pub image_virtual_start:u64,pub image_size:u64,pub pages:Vec<PlannedPage>,pub segment_copies:Vec<SegmentCopy>,pub architecture:ElfArchitecture}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum LoadPlanError{NoLoadableSegments,InvalidPageSize,AddressOverflow,InvalidAlignment,SegmentOutsideImage,WritableExecutablePage,InvalidLoadBias,InvalidSegment,ImageTooLarge}
fn align_down(v:u64,a:u64)->u64{v&!(a-1)}
fn align_up(v:u64,a:u64)->Result<u64,LoadPlanError>{v.checked_add(a-1).map(|x|x&!(a-1)).ok_or(LoadPlanError::AddressOverflow)}
/// Produces a deterministic 16 KiB mapping plan. load_bias is added to ELF virtual addresses by the future mapper.
pub fn plan_native_image(info:&ElfProgramInfo,load_bias:u64)->Result<NativeImagePlan,LoadPlanError>{
 if IOS_PAGE_SIZE==0||!IOS_PAGE_SIZE.is_power_of_two(){return Err(LoadPlanError::InvalidPageSize);}
 let loads:Vec<_>=info.segments.iter().filter(|s|s.memory_size>0).collect();
 if loads.is_empty(){return Err(LoadPlanError::NoLoadableSegments);}
 let(mut image_start,mut image_end)=(u64::MAX,0u64);
 for s in &loads{
  if s.file_size>s.memory_size{return Err(LoadPlanError::InvalidSegment);}
  // Validate all address arithmetic before alignment so overflow is reported
  // deterministically even when the overflowing address is also misaligned.
  let end=s.virtual_address.checked_add(s.memory_size).ok_or(LoadPlanError::AddressOverflow)?;
  s.file_offset.checked_add(s.file_size).ok_or(LoadPlanError::AddressOverflow)?;
  s.virtual_address.checked_add(s.file_size).ok_or(LoadPlanError::AddressOverflow)?;
  if s.alignment>1&&(!s.alignment.is_power_of_two()||s.virtual_address%s.alignment!=s.file_offset%s.alignment){return Err(LoadPlanError::InvalidAlignment);}
  image_start=image_start.min(align_down(s.virtual_address,IOS_PAGE_SIZE));
  image_end=image_end.max(align_up(end,IOS_PAGE_SIZE)?);
 }
 let image_size=image_end.checked_sub(image_start).ok_or(LoadPlanError::AddressOverflow)?;
 if image_size/IOS_PAGE_SIZE > 262_144 {return Err(LoadPlanError::ImageTooLarge);}
 if load_bias.checked_add(image_start).is_none()||load_bias.checked_add(image_end).is_none(){return Err(LoadPlanError::InvalidLoadBias);}
 let mut page_map=BTreeMap::<u64,PageProtection>::new();
 let mut copies=Vec::with_capacity(loads.len());
 for s in loads{
  let end=s.virtual_address.checked_add(s.memory_size).ok_or(LoadPlanError::AddressOverflow)?;
  let first=align_down(s.virtual_address,IOS_PAGE_SIZE);let last=align_up(end,IOS_PAGE_SIZE)?;
  let protection=PageProtection::from_elf(s.flags);let mut page=first;
  while page<last{page_map.entry(page).or_default().merge(protection)?;page=page.checked_add(IOS_PAGE_SIZE).ok_or(LoadPlanError::AddressOverflow)?;}
  let offset=s.virtual_address.checked_sub(image_start).ok_or(LoadPlanError::SegmentOutsideImage)?;
  if offset.checked_add(s.memory_size).filter(|e|*e<=image_size).is_none(){return Err(LoadPlanError::SegmentOutsideImage);}
  copies.push(SegmentCopy{file_offset:s.file_offset,image_offset:offset,file_size:s.file_size,memory_size:s.memory_size});
 }
 let pages=page_map.into_iter().map(|(address,protection)|PlannedPage{image_offset:address-image_start,protection}).collect();
 Ok(NativeImagePlan{load_bias,image_virtual_start:image_start,image_size,pages,segment_copies:copies,architecture:info.identity.architecture})
}

/// Upper bound for a single materialized guest image. The limit prevents a
/// malformed ELF with huge sparse virtual ranges from exhausting host memory.
pub const MAX_NATIVE_IMAGE_BYTES:u64=512*1024*1024;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum LoadImageError{ImageTooLarge,AllocationFailed,InvalidSegment,SegmentOutOfBounds}

/// Materializes PT_LOAD file bytes into a zero-initialized guest image buffer.
/// BSS is therefore represented by the zero-filled memory tail. This is data
/// preparation only: it does not map executable pages or invoke guest code.
pub fn materialize_native_image(bytes:&[u8],plan:&NativeImagePlan)->Result<Vec<u8>,LoadImageError>{
 if plan.image_size>MAX_NATIVE_IMAGE_BYTES{return Err(LoadImageError::ImageTooLarge);}
 let image_len=usize::try_from(plan.image_size).map_err(|_|LoadImageError::ImageTooLarge)?;
 let mut image=Vec::new();
 image.try_reserve_exact(image_len).map_err(|_|LoadImageError::AllocationFailed)?;
 image.resize(image_len,0);
 for segment in &plan.segment_copies{
  if segment.file_size>segment.memory_size{return Err(LoadImageError::InvalidSegment);}
  let src_start=usize::try_from(segment.file_offset).map_err(|_|LoadImageError::SegmentOutOfBounds)?;
  let src_len=usize::try_from(segment.file_size).map_err(|_|LoadImageError::SegmentOutOfBounds)?;
  let dst_start=usize::try_from(segment.image_offset).map_err(|_|LoadImageError::SegmentOutOfBounds)?;
  let dst_len=usize::try_from(segment.memory_size).map_err(|_|LoadImageError::SegmentOutOfBounds)?;
  let src_end=src_start.checked_add(src_len).filter(|end|*end<=bytes.len()).ok_or(LoadImageError::SegmentOutOfBounds)?;
  let dst_end=dst_start.checked_add(dst_len).filter(|end|*end<=image.len()).ok_or(LoadImageError::SegmentOutOfBounds)?;
  image[dst_start..dst_start+src_len].copy_from_slice(&bytes[src_start..src_end]);
  // Keep the full memory extent validated; the remaining bytes stay zero for BSS.
  let _=dst_end;
 }
 Ok(image)
}

#[cfg(test)]mod tests{
 use super::*;use stride_elf_inspector::{ElfByteOrder,ElfClass,ElfIdentity,LoadSegment};
 fn image(segments:Vec<LoadSegment>)->ElfProgramInfo{ElfProgramInfo{identity:ElfIdentity{class:ElfClass::Elf64,architecture:ElfArchitecture::Aarch64},byte_order:ElfByteOrder::Little,entry:0x1000,segments,dynamic_segment:None}}
 fn seg(offset:u64,address:u64,file:u64,memory:u64,flags:u32)->LoadSegment{LoadSegment{file_offset:offset,virtual_address:address,file_size:file,memory_size:memory,flags,alignment:0x1000}}
 #[test]fn plans_ios_pages(){let i=image(vec![seg(0,0x1000,0x1800,0x1800,5),seg(0x4000,0x5000,0x800,0x2000,6)]);let p=plan_native_image(&i,0x1_0000_0000).unwrap();assert_eq!(p.image_virtual_start,0);assert_eq!(p.image_size,IOS_PAGE_SIZE*2);assert_eq!(p.pages.len(),2);assert!(p.pages.iter().all(|x|!(x.protection.writable&&x.protection.executable)));}
 #[test]fn rejects_writable_executable_page(){let i=image(vec![seg(0,0x1000,0x1000,0x1000,5),seg(0x1000,0x2000,0x1000,0x1000,6)]);assert_eq!(plan_native_image(&i,0),Err(LoadPlanError::WritableExecutablePage));}
 #[test]fn rejects_bad_alignment(){let mut s=seg(0,0x1000,0x100,0x100,5);s.alignment=3;assert_eq!(plan_native_image(&image(vec![s]),0),Err(LoadPlanError::InvalidAlignment));}
 #[test]fn rejects_empty_image(){assert_eq!(plan_native_image(&image(vec![]),0),Err(LoadPlanError::NoLoadableSegments));}
 #[test]fn rejects_address_overflow(){let i=image(vec![seg(0,u64::MAX-10,0,32,4)]);assert_eq!(plan_native_image(&i,0),Err(LoadPlanError::AddressOverflow));}
}

// Bounded relocation application for the small, architecture-neutral subset
// needed by Android shared objects. This patches a data buffer only; it does
// not create executable memory or run guest instructions.
use stride_elf_inspector::{DynamicSymbol, ElfByteOrder, ElfDynamicInfo, Relocation, RelocationKind};

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum RelocationError {
    UnsupportedArchitecture,
    UnsupportedByteOrder,
    UnsupportedRelocation(u32),
    InvalidSymbolIndex,
    UnresolvedSymbol,
    TargetOutOfBounds,
    AddressOverflow,
    ValueOutOfRange,
    InvalidImage,
}
pub type SymbolResolver<'a> = dyn FnMut(&DynamicSymbol) -> Option<u64> + 'a;

fn relocation_target(plan:&NativeImagePlan,rel:&Relocation,width:usize,image_len:usize)->Result<usize,RelocationError>{
    let offset=rel.offset.checked_sub(plan.image_virtual_start).ok_or(RelocationError::TargetOutOfBounds)?;
    let start=usize::try_from(offset).map_err(|_|RelocationError::TargetOutOfBounds)?;
    let end=start.checked_add(width).filter(|e|*e<=image_len).ok_or(RelocationError::TargetOutOfBounds)?;
    let _=end;
    Ok(start)
}
fn signed_add(base:u64,addend:i64)->Result<u64,RelocationError>{
    if addend>=0 { base.checked_add(addend as u64).ok_or(RelocationError::AddressOverflow) }
    else { base.checked_sub(addend.unsigned_abs()).ok_or(RelocationError::AddressOverflow) }
}

/// Applies supported Android ARM64/ARM32 relocations to a previously materialized
/// image. The resolver must return the guest address of undefined imports.
/// Defined symbols are resolved from their ELF value plus load bias.
/// Unsupported relocation kinds fail closed instead of being silently skipped.
pub fn apply_relocations(
    image:&mut [u8],
    plan:&NativeImagePlan,
    dynamic:&ElfDynamicInfo,
    byte_order:ElfByteOrder,
    resolver:&mut SymbolResolver<'_>,
)->Result<usize,RelocationError>{
    if byte_order!=ElfByteOrder::Little { return Err(RelocationError::UnsupportedByteOrder); }
    let is64=match plan.architecture {
        ElfArchitecture::Aarch64=>true,
        ElfArchitecture::Arm=>false,
        _=>return Err(RelocationError::UnsupportedArchitecture),
    };
    let expected_len=usize::try_from(plan.image_size).map_err(|_|RelocationError::InvalidImage)?;
    if expected_len != image.len() { return Err(RelocationError::InvalidImage); }
    // Stage every patch first. A late unsupported relocation must not leave a
    // half-relocated image behind.
    let mut patches:Vec<(usize,Vec<u8>)>=Vec::with_capacity(dynamic.relocations.len());
    let mut applied=0usize;
    for rel in &dynamic.relocations {
        let width=if is64 {8usize}else{4usize};
        let target=relocation_target(plan,rel,width,image.len())?;
        let implicit=if is64 {
            u64::from_le_bytes(image[target..target+8].try_into().map_err(|_|RelocationError::TargetOutOfBounds)?)
        } else {
            u32::from_le_bytes(image[target..target+4].try_into().map_err(|_|RelocationError::TargetOutOfBounds)?) as u64
        };
        let addend=rel.addend.unwrap_or(implicit as i64);
        let sym=dynamic.symbols.get(rel.symbol_index as usize).ok_or(RelocationError::InvalidSymbolIndex)?;
        let (relative,absolute,glob,jump)=if is64 {(1027,257,1025,1026)} else {(23,2,21,22)};
        let value=if rel.relocation_type==relative {
            if rel.symbol_index!=0 { return Err(RelocationError::InvalidSymbolIndex); }
            signed_add(plan.load_bias,addend)?
        } else if [absolute,glob,jump].contains(&rel.relocation_type) {
            let symbol_address=if sym.section_index==0 {
                resolver(sym).ok_or(RelocationError::UnresolvedSymbol)?
            } else {
                plan.load_bias.checked_add(sym.value).ok_or(RelocationError::AddressOverflow)?
            };
            signed_add(symbol_address,addend)?
        } else {
            return Err(RelocationError::UnsupportedRelocation(rel.relocation_type));
        };
        let patch=if is64 { value.to_le_bytes().to_vec() } else {
            u32::try_from(value).map_err(|_|RelocationError::ValueOutOfRange)?.to_le_bytes().to_vec()
        };
        patches.push((target,patch));
        applied+=1;
    }
    for (target,patch) in patches { image[target..target+patch.len()].copy_from_slice(&patch); }
    Ok(applied)
}

#[cfg(test)]
mod relocation_tests {
    use super::*;
    use stride_elf_inspector::{DynamicSymbol,Relocation};
    fn plan()->NativeImagePlan {
        NativeImagePlan{load_bias:0x1000,image_virtual_start:0x2000,image_size:0x4000,pages:vec![],segment_copies:vec![],architecture:ElfArchitecture::Aarch64}
    }
    fn sym(index:u32,value:u64,section_index:u16)->DynamicSymbol {
        DynamicSymbol{index,name:"test".into(),value,size:8,binding:1,symbol_type:1,section_index}
    }
    #[test]
    fn applies_aarch64_relative_rela() {
        let mut image=vec![0u8;0x4000];
        let dyninfo=ElfDynamicInfo{symbols:vec![sym(0,0,0)],relocations:vec![Relocation{kind:RelocationKind::Rela,offset:0x2010,symbol_index:0,relocation_type:1027,addend:Some(0x88)}],needed_libraries:vec![],dynamic_tags:vec![]};
        let mut resolver=|_:&DynamicSymbol|None;
        assert_eq!(apply_relocations(&mut image,&plan(),&dyninfo,ElfByteOrder::Little,&mut resolver),Ok(1));
        assert_eq!(u64::from_le_bytes(image[0x10..0x18].try_into().unwrap()),0x1088);
    }
    #[test]
    fn resolves_undefined_aarch64_jump_slot() {
        let mut image=vec![0u8;0x4000];
        let dyninfo=ElfDynamicInfo{symbols:vec![sym(0,0,0)],relocations:vec![Relocation{kind:RelocationKind::PltRela,offset:0x2020,symbol_index:0,relocation_type:1026,addend:Some(4)}],needed_libraries:vec![],dynamic_tags:vec![]};
        let mut resolver=|_:&DynamicSymbol|Some(0x5000);
        assert_eq!(apply_relocations(&mut image,&plan(),&dyninfo,ElfByteOrder::Little,&mut resolver),Ok(1));
        assert_eq!(u64::from_le_bytes(image[0x20..0x28].try_into().unwrap()),0x5004);
    }
    #[test]
    fn rejects_unknown_relocation_without_mutating_target() {
        let mut image=vec![0u8;0x4000];
        let dyninfo=ElfDynamicInfo{symbols:vec![sym(0,0,0)],relocations:vec![Relocation{kind:RelocationKind::Rela,offset:0x2030,symbol_index:0,relocation_type:9999,addend:Some(0)}],needed_libraries:vec![],dynamic_tags:vec![]};
        let mut resolver=|_:&DynamicSymbol|None;
        assert_eq!(apply_relocations(&mut image,&plan(),&dyninfo,ElfByteOrder::Little,&mut resolver),Err(RelocationError::UnsupportedRelocation(9999)));
        assert_eq!(&image[0x30..0x38],&[0;8]);
    }
    #[test]
    fn relocation_failure_does_not_partially_patch_image() {
        let mut image=vec![0u8;0x4000];
        let dyninfo=ElfDynamicInfo{symbols:vec![sym(0,0,0)],relocations:vec![
            Relocation{kind:RelocationKind::Rela,offset:0x2010,symbol_index:0,relocation_type:1027,addend:Some(0x88)},
            Relocation{kind:RelocationKind::Rela,offset:0x2020,symbol_index:0,relocation_type:9999,addend:Some(0)},
        ],needed_libraries:vec![],dynamic_tags:vec![]};
        let mut resolver=|_:&DynamicSymbol|None;
        assert_eq!(apply_relocations(&mut image,&plan(),&dyninfo,ElfByteOrder::Little,&mut resolver),Err(RelocationError::UnsupportedRelocation(9999)));
        assert_eq!(&image[0x10..0x18],&[0;8]);
    }
    #[test]
    fn rejects_relocation_outside_image() {
        let mut image=vec![0u8;0x4000];
        let dyninfo=ElfDynamicInfo{symbols:vec![sym(0,0,0)],relocations:vec![Relocation{kind:RelocationKind::Rela,offset:0x7000,symbol_index:0,relocation_type:1027,addend:Some(0)}],needed_libraries:vec![],dynamic_tags:vec![]};
        let mut resolver=|_:&DynamicSymbol|None;
        assert_eq!(apply_relocations(&mut image,&plan(),&dyninfo,ElfByteOrder::Little,&mut resolver),Err(RelocationError::TargetOutOfBounds));
    }
}


// Dependency ordering for Android DT_NEEDED graphs. The caller supplies the
// archive/SONAME index; this routine validates the full dependency closure and
// returns libraries in dependency-first constructor order.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug,Clone,PartialEq,Eq)]
pub enum DependencyError {
    MissingLibrary(String),
    DependencyCycle(Vec<String>),
}
fn visit_dependency(
    name:&str,
    graph:&BTreeMap<String,Vec<String>>,
    visiting:&mut Vec<String>,
    visited:&mut BTreeSet<String>,
    ordered:&mut Vec<String>,
)->Result<(),DependencyError>{
    if visited.contains(name) { return Ok(()); }
    if let Some(position)=visiting.iter().position(|item|item==name) {
        let mut cycle=visiting[position..].to_vec();
        cycle.push(name.to_string());
        return Err(DependencyError::DependencyCycle(cycle));
    }
    let dependencies=graph.get(name).ok_or_else(||DependencyError::MissingLibrary(name.to_string()))?;
    visiting.push(name.to_string());
    for dependency in dependencies {
        visit_dependency(dependency,graph,visiting,visited,ordered)?;
    }
    visiting.pop();
    visited.insert(name.to_string());
    ordered.push(name.to_string());
    Ok(())
}
/// Returns a dependency-first load/constructor order for the root and its full
/// transitive closure. It does not open archives, map memory, or run constructors.
pub fn dependency_load_order(
    root:&str,
    graph:&BTreeMap<String,Vec<String>>,
)->Result<Vec<String>,DependencyError>{
    let mut visiting=Vec::new();
    let mut visited=BTreeSet::new();
    let mut ordered=Vec::new();
    visit_dependency(root,graph,&mut visiting,&mut visited,&mut ordered)?;
    Ok(ordered)
}

#[cfg(test)]
mod dependency_tests {
    use super::*;
    #[test]
    fn returns_transitive_dependency_first_order() {
        let graph=BTreeMap::from([
            ("libgame.so".to_string(),vec!["libcocos.so".to_string(),"libandroid.so".to_string()]),
            ("libcocos.so".to_string(),vec!["libc.so".to_string()]),
            ("libandroid.so".to_string(),vec!["libc.so".to_string()]),
            ("libc.so".to_string(),vec![]),
        ]);
        assert_eq!(dependency_load_order("libgame.so",&graph),Ok(vec![
            "libc.so".to_string(),"libcocos.so".to_string(),"libandroid.so".to_string(),"libgame.so".to_string()
        ]));
    }
    #[test]
    fn reports_missing_transitive_library() {
        let graph=BTreeMap::from([("libgame.so".to_string(),vec!["libmissing.so".to_string()])]);
        assert_eq!(dependency_load_order("libgame.so",&graph),Err(DependencyError::MissingLibrary("libmissing.so".to_string())));
    }
    #[test]
    fn rejects_dependency_cycles() {
        let graph=BTreeMap::from([
            ("liba.so".to_string(),vec!["libb.so".to_string()]),
            ("libb.so".to_string(),vec!["liba.so".to_string()]),
        ]);
        assert_eq!(dependency_load_order("liba.so",&graph),Err(DependencyError::DependencyCycle(vec!["liba.so".to_string(),"libb.so".to_string(),"liba.so".to_string()])));
    }
}
