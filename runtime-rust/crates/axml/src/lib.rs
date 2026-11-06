//! Bounds-checked Android binary XML chunk and string-pool reader.
//! This stage validates and decodes string-pool entries; it does not yet resolve manifest semantics.
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum AxmlError { Truncated, InvalidMagic, InvalidFileSize, InvalidChunk, InvalidStringPool, InvalidStringIndex, UnsupportedStringEncoding }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct StringPool { pub strings: Vec<String> }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct XmlDocument { pub file_size:u32, pub chunk_count:usize, pub string_pool:Option<StringPool> }
fn u16le(b:&[u8],o:usize)->Result<u16,AxmlError>{let s=b.get(o..o.checked_add(2).ok_or(AxmlError::Truncated)?).ok_or(AxmlError::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32le(b:&[u8],o:usize)->Result<u32,AxmlError>{let s=b.get(o..o.checked_add(4).ok_or(AxmlError::Truncated)?).ok_or(AxmlError::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn len8(b:&[u8],p:&mut usize)->Result<usize,AxmlError>{let x=*b.get(*p).ok_or(AxmlError::Truncated)? as usize;*p+=1;if x&0x80==0{Ok(x)}else{let y=*b.get(*p).ok_or(AxmlError::Truncated)? as usize;*p+=1;Ok(((x&0x7f)<<8)|y)}}
fn len16(b:&[u8],p:&mut usize)->Result<usize,AxmlError>{let x=u16le(b,*p)? as usize;*p+=2;if x&0x8000==0{Ok(x)}else{let y=u16le(b,*p)? as usize;*p+=2;Ok(((x&0x7fff)<<16)|y)}}
fn parse_pool(b:&[u8],start:usize,size:usize)->Result<StringPool,AxmlError>{
 let hs=u16le(b,start)? as usize;let _=u16le(b,start+2)?;let count=u32le(b,start+8)? as usize;let styles=u32le(b,start+12)? as usize;let flags=u32le(b,start+16)?;let strings_start=u32le(b,start+20)? as usize;
 if hs<28||hs>size||count>1_000_000{return Err(AxmlError::InvalidStringPool);}
 let offsets_start=start.checked_add(hs).ok_or(AxmlError::InvalidStringPool)?;
 let offsets_end=offsets_start.checked_add(count.checked_mul(4).ok_or(AxmlError::InvalidStringPool)?).filter(|e|*e<=start+size).ok_or(AxmlError::InvalidStringPool)?;
 let _=offsets_end;
 let utf8=flags&0x100!=0;let mut strings=Vec::with_capacity(count);
 for i in 0..count {
  let rel=u32le(b,offsets_start+i*4)? as usize;
  let mut p=start.checked_add(strings_start).and_then(|v|v.checked_add(rel)).filter(|v|*v<start+size).ok_or(AxmlError::InvalidStringPool)?;
  if utf8 {
   let _utf16_len=len8(b,&mut p)?;let bytes_len=len8(b,&mut p)?;
   let end=p.checked_add(bytes_len).filter(|e|*e<start+size).ok_or(AxmlError::InvalidStringPool)?;
   let raw=&b[p..end];if b.get(end)!=Some(&0){return Err(AxmlError::InvalidStringPool);}
   strings.push(std::str::from_utf8(raw).map_err(|_|AxmlError::InvalidStringPool)?.to_owned());
  } else {
   let units=len16(b,&mut p)?;let end=p.checked_add(units.checked_mul(2).ok_or(AxmlError::InvalidStringPool)?).filter(|e|e+2<=start+size).ok_or(AxmlError::InvalidStringPool)?;
   let words=(0..units).map(|n|u16le(b,p+n*2)).collect::<Result<Vec<_>,_>>()?;
   if u16le(b,end)?!=0{return Err(AxmlError::InvalidStringPool);}
   strings.push(String::from_utf16(&words).map_err(|_|AxmlError::InvalidStringPool)?);
  }
 }
 let _=styles;
 Ok(StringPool{strings})
}
/// Walks an Android binary XML document, validating chunk bounds and decoding its string pool.
pub fn inspect_binary_xml(b:&[u8])->Result<XmlDocument,AxmlError>{
 if b.len()<8{return Err(AxmlError::Truncated);}
 if u16le(b,0)?!=3||u16le(b,2)?!=8{return Err(AxmlError::InvalidMagic);}
 let file_size=u32le(b,4)?;if file_size<8||file_size as usize>b.len(){return Err(AxmlError::InvalidFileSize);}
 let mut p=8usize;let mut chunks=0;let mut pool=None;
 while p<(file_size as usize){
  if p+8>file_size as usize{return Err(AxmlError::Truncated);}
  let kind=u16le(b,p)?;let header=u16le(b,p+2)? as usize;let size=u32le(b,p+4)? as usize;
  if header<8||size<header||p.checked_add(size).filter(|e|*e<=file_size as usize).is_none(){return Err(AxmlError::InvalidChunk);}
  if kind==1 {if pool.is_some(){return Err(AxmlError::InvalidStringPool);}pool=Some(parse_pool(b,p,size)?);}
  p+=size;chunks+=1;
 }
 if p!=file_size as usize{return Err(AxmlError::InvalidChunk);}
 Ok(XmlDocument{file_size,chunk_count:chunks,string_pool:pool})
}
#[cfg(test)] mod tests{
 use super::*;
 #[test]fn rejects_plain_xml(){assert_eq!(inspect_binary_xml(b"<manifest/>"),Err(AxmlError::InvalidMagic));}
 #[test]fn rejects_truncated_chunk(){let b=vec![3,0,8,0,16,0,0,0,1,0,8,0,20,0,0,0];assert_eq!(inspect_binary_xml(&b),Err(AxmlError::InvalidChunk));}
 #[test]fn accepts_empty_xml_container(){let b=[3,0,8,0,8,0,0,0];let d=inspect_binary_xml(&b).unwrap();assert_eq!(d.chunk_count,0);}
}


// Android manifest semantic extraction over the decoded AXML string pool.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ManifestComponent { pub kind:String, pub name:String, pub exported:Option<bool>, pub has_launcher_intent:bool }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ManifestMetadata {
 pub package_name:Option<String>, pub version_name:Option<String>, pub version_code:Option<u32>,
 pub min_sdk:Option<u32>, pub target_sdk:Option<u32>, pub permissions:Vec<String>,
 pub application_name:Option<String>, pub components:Vec<ManifestComponent>, pub launcher_activity:Option<String>
}
fn string_at(pool:&StringPool,index:u32)->Result<Option<String>,AxmlError>{
 if index==u32::MAX{return Ok(None);}pool.strings.get(index as usize).cloned().map(Some).ok_or(AxmlError::InvalidStringIndex)
}
fn typed_attr(b:&[u8],p:usize,pool:&StringPool)->Result<Option<String>,AxmlError>{
 let raw=u32le(b,p+8)?;if raw!=u32::MAX{return string_at(pool,raw);}
 let data_type=*b.get(p+15).ok_or(AxmlError::Truncated)?;let data=u32le(b,p+16)?;
 match data_type {3=>string_at(pool,data),0x10|0x11=>Ok(Some(data.to_string())),0x12=>Ok(Some((data!=0).to_string())),_=>Ok(None)}
}
/// Extracts package identity, SDK levels, permissions, application and launchable components.
/// Namespace prefixes are intentionally ignored; Android attribute local names are used.
pub fn parse_manifest(bytes:&[u8])->Result<ManifestMetadata,AxmlError>{
 let doc=inspect_binary_xml(bytes)?;let pool=doc.string_pool.ok_or(AxmlError::InvalidStringPool)?;
 let end=doc.file_size as usize;let mut p=8usize;let mut out=ManifestMetadata{package_name:None,version_name:None,version_code:None,min_sdk:None,target_sdk:None,permissions:Vec::new(),application_name:None,components:Vec::new(),launcher_activity:None};
 let mut stack:Vec<String>=Vec::new();let mut component_stack:Vec<Option<usize>>=Vec::new();
 let mut intent_filters:Vec<(usize,bool,bool)>=Vec::new();
 while p<end {
  let kind=u16le(bytes,p)?;let size=u32le(bytes,p+4)? as usize;
  if kind==0x0102 {
   if size<36{return Err(AxmlError::InvalidChunk);}
   let name=string_at(&pool,u32le(bytes,p+20)?)?.ok_or(AxmlError::InvalidStringIndex)?;
   let attr_start=u16le(bytes,p+24)? as usize;let attr_size=u16le(bytes,p+26)? as usize;let attr_count=u16le(bytes,p+28)? as usize;
   if attr_size<20{return Err(AxmlError::InvalidChunk);}
   let base=p.checked_add(16).and_then(|v|v.checked_add(attr_start)).ok_or(AxmlError::InvalidChunk)?;
   let mut attrs=std::collections::BTreeMap::<String,String>::new();
   for i in 0..attr_count {
    let a=base.checked_add(i.checked_mul(attr_size).ok_or(AxmlError::InvalidChunk)?).ok_or(AxmlError::InvalidChunk)?;
    if a.checked_add(20).filter(|v|*v<=p+size).is_none(){return Err(AxmlError::InvalidChunk);}
    let an=string_at(&pool,u32le(bytes,a+4)?)?.ok_or(AxmlError::InvalidStringIndex)?;
    if let Some(value)=typed_attr(bytes,a,&pool)?{attrs.insert(an,value);}
   }
   let mut component_index=None;
   match name.as_str() {
    "manifest"=>{out.package_name=attrs.get("package").cloned();out.version_name=attrs.get("versionName").cloned();out.version_code=attrs.get("versionCode").and_then(|v|v.parse().ok());},
    "uses-sdk"=>{out.min_sdk=attrs.get("minSdkVersion").and_then(|v|v.parse().ok());out.target_sdk=attrs.get("targetSdkVersion").and_then(|v|v.parse().ok());},
    "uses-permission"|"uses-permission-sdk-23"|"uses-permission-sdk-m"=>{if let Some(v)=attrs.get("name"){if !out.permissions.contains(v){out.permissions.push(v.clone());}}},
    "application"=>{out.application_name=attrs.get("name").cloned();},
    "intent-filter"=>{if let Some(Some(idx))=component_stack.last(){intent_filters.push((*idx,false,false));}},
    "activity"|"activity-alias"|"service"|"receiver"|"provider"=>{
     if let Some(n)=attrs.get("name") {
      let idx=out.components.len();out.components.push(ManifestComponent{kind:name.clone(),name:n.clone(),exported:attrs.get("exported").map(|v|v=="true"),has_launcher_intent:false});component_index=Some(idx);
     }
    },
    "action" if attrs.get("name").map(String::as_str)==Some("android.intent.action.MAIN")=>{
      if let Some(filter)=intent_filters.last_mut(){filter.1=true;}
    },
    "category" if attrs.get("name").map(String::as_str)==Some("android.intent.category.LAUNCHER")=>{
      if let Some(filter)=intent_filters.last_mut(){filter.2=true;}
    },
    _=>{}
   }
   // Keep the nearest component association for nested intent-filter/action/category nodes.
   if component_index.is_none(){component_index=component_stack.last().copied().flatten();}
   stack.push(name);component_stack.push(component_index);
  } else if kind==0x0103 {
   if size<24{return Err(AxmlError::InvalidChunk);}
   let name=string_at(&pool,u32le(bytes,p+20)?)?.ok_or(AxmlError::InvalidStringIndex)?;
   if stack.pop().as_deref()!=Some(name.as_str()){return Err(AxmlError::InvalidChunk);}
   component_stack.pop();
   if name=="intent-filter" {
    if let Some((idx,has_main,has_launcher))=intent_filters.pop(){
     if has_main&&has_launcher{out.components[idx].has_launcher_intent=true;}
    }
   }
  }
  p+=size;
 }
 out.launcher_activity=out.components.iter().find(|c|c.kind=="activity"&&c.has_launcher_intent).map(|c|c.name.clone());
 Ok(out)
}

#[cfg(test)]
mod manifest_tests {
 use super::*;
 #[test] fn rejects_manifest_without_string_pool(){let b=[3,0,8,0,8,0,0,0];assert_eq!(parse_manifest(&b),Err(AxmlError::InvalidStringPool));}
}
