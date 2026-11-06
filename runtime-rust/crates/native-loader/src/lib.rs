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
  if s.alignment>1&&(!s.alignment.is_power_of_two()||s.virtual_address%s.alignment!=s.file_offset%s.alignment){return Err(LoadPlanError::InvalidAlignment);}
  let end=s.virtual_address.checked_add(s.memory_size).ok_or(LoadPlanError::AddressOverflow)?;
  image_start=image_start.min(align_down(s.virtual_address,IOS_PAGE_SIZE));
  image_end=image_end.max(align_up(end,IOS_PAGE_SIZE)?);
  s.file_offset.checked_add(s.file_size).ok_or(LoadPlanError::AddressOverflow)?;
  s.virtual_address.checked_add(s.file_size).ok_or(LoadPlanError::AddressOverflow)?;
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