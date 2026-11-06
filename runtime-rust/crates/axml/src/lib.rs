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
