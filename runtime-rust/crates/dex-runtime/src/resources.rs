//! Bounds-checked Android resources.arsc value resolver (simple scalar entries).
use std::collections::BTreeMap;
#[derive(Debug,Default,Clone)]
pub struct ResourceCatalog { strings:BTreeMap<u32,String>, colors:BTreeMap<u32,u32>, files:BTreeMap<u32,String> }
impl ResourceCatalog {
 pub fn parse(data:&[u8])->Result<Self,String>{
  fn u16at(b:&[u8],p:usize)->Result<u16,String>{Ok(u16::from_le_bytes(b.get(p..p+2).ok_or("truncated u16")?.try_into().unwrap()))}
  fn u32at(b:&[u8],p:usize)->Result<u32,String>{Ok(u32::from_le_bytes(b.get(p..p+4).ok_or("truncated u32")?.try_into().unwrap()))}
  fn pool(b:&[u8],base:usize,size:usize)->Result<Vec<String>,String>{
   let hs=u16at(b,base+2)? as usize;let count=u32at(b,base+8)? as usize;let flags=u32at(b,base+16)?;
   if count>1_000_000{return Err("resource string pool exceeds bound".into());}
   let strings_start=u32at(b,base+20)? as usize;let utf8=flags&0x100!=0;let mut out=Vec::with_capacity(count);
   for i in 0..count{let off=u32at(b,base+hs+i*4)? as usize;let mut p=base+strings_start+off;
    if utf8{
     let read_len=|b:&[u8],p:&mut usize|->Result<usize,String>{let a=*b.get(*p).ok_or("truncated utf8 length")? as usize;*p+=1;if a&0x80!=0{let z=*b.get(*p).ok_or("truncated utf8 length")? as usize;*p+=1;Ok(((a&0x7f)<<8)|z)}else{Ok(a)}};
     let _utf16=read_len(b,&mut p)?;let n=read_len(b,&mut p)?;let bytes=b.get(p..p+n).ok_or("truncated resource string")?;out.push(String::from_utf8_lossy(bytes).into_owned());
    }else{
     let a=u16at(b,p)? as usize;p+=2;let units=if a&0x8000!=0{(((a&0x7fff)<<16)|u16at(b,p)? as usize)}else{a};if a&0x8000!=0{p+=2;}
     let mut v=Vec::with_capacity(units);for _ in 0..units{v.push(u16at(b,p)?);p+=2;}out.push(String::from_utf16_lossy(&v));
    }
   }let _=size;Ok(out)
  }
  if data.len()<12||u16at(data,0)?!=0x0002{return Err("invalid resources.arsc table header".into());}
  let total=u32at(data,4)? as usize;if total>data.len()||total<12{return Err("invalid resources.arsc size".into());}
  let mut catalog=Self::default();let mut global=Vec::<String>::new();let mut pos=u16at(data,2)? as usize;
  while pos+8<=total{
   let kind=u16at(data,pos)?;let hs=u16at(data,pos+2)? as usize;let size=u32at(data,pos+4)? as usize;
   if size<hs||pos.checked_add(size).filter(|e|*e<=total).is_none(){return Err("resource chunk outside table bounds".into());}
   if kind==0x0001{global=pool(data,pos,size)?;}
   else if kind==0x0200{
    let package_id=u32at(data,pos+8)?;let package_end=pos+size;let mut q=pos+hs;
    while q+8<=package_end{
     let kt=u16at(data,q)?;let kh=u16at(data,q+2)? as usize;let ks=u32at(data,q+4)? as usize;
     if ks<kh||q+ks>package_end{return Err("invalid package resource chunk".into());}
     if kt==0x0201{
      let type_id=*data.get(q+8).ok_or("missing type id")? as u32;let entry_count=u32at(data,q+12)? as usize;let entries_start=u32at(data,q+16)? as usize;
      if entry_count>1_000_000{return Err("resource entry count exceeds bound".into());}
      for index in 0..entry_count{
       let off=u32at(data,q+kh+index*4)?;if off==u32::MAX{continue;}
       let ep=q+entries_start+off as usize;if ep+8>q+ks{continue;}
       let entry_size=u16at(data,ep)? as usize;let flags=u16at(data,ep+2)?;
       if flags&1!=0||entry_size<8{continue;}
       let vp=ep+entry_size;if vp+8>q+ks{continue;}
       let value_size=u16at(data,vp)?;if value_size<8{continue;}
       let dtype=data[vp+3];let value=u32at(data,vp+4)?;
       let id=((package_id&0xff)<<24)|((type_id&0xff)<<16)|(index as u32&0xffff);
       match dtype{
        0x03=>if let Some(s)=global.get(value as usize){if s.starts_with("res/")||s.starts_with("assets/"){catalog.files.insert(id,s.clone());}else{catalog.strings.insert(id,s.clone());}},
        0x1c..=0x1f=>{catalog.colors.insert(id,value);},
        _=>{}
       }
      }
     }
     q+=ks;
    }
   }
   pos+=size;
  }
  Ok(catalog)
 }
 pub fn string(&self,id:u32)->Option<&str>{self.strings.get(&id).map(String::as_str)}
 pub fn color(&self,id:u32)->Option<u32>{self.colors.get(&id).copied()}
 pub fn file(&self,id:u32)->Option<&str>{self.files.get(&id).map(String::as_str)}
 pub fn file_entries(&self)->Vec<(u32,String)>{self.files.iter().map(|(id,path)|(*id,path.clone())).collect()}
}
#[cfg(test)]mod tests{use super::*;#[test]fn rejects_truncated_table(){assert!(ResourceCatalog::parse(&[2,0,12,0,12,0,0,0]).is_err());}}
