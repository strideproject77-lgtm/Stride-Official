//! Android Paint JNI bridge with managed per-object state.
use crate::jni::JniNativeRegistry;
use crate::types::Value;

fn id(a:&[Value])->Result<u32,String>{a.first().and_then(Value::as_object).filter(|v|*v!=0).ok_or_else(||"Paint receiver is null or invalid".into())}
fn int(a:&[Value],i:usize,d:i32)->i32{a.get(i).map(Value::as_int).unwrap_or(d)}
fn flt(a:&[Value],i:usize,d:f32)->f32{a.get(i).map(Value::as_float).unwrap_or(d)}
fn flag(v:bool)->Value{Value::Int(if v{1}else{0})}

fn register(j:&mut JniNativeRegistry){
 for proto in ["()V","(I)V","(Landroid/graphics/Paint;)V"]{
  j.register("Landroid/graphics/Paint;","<init>",proto,|e,a|{
   let p=id(a)?;
   let mut color=0xff000000u32; let mut width=1.0; let mut size=14.0; let mut flags=1;
   if let Some(src)=a.get(1).and_then(Value::as_object){
    color=e.get_object_field(src,"mColor")?.as_int() as u32;
    width=e.get_object_field(src,"mStrokeWidth")?.as_float();
    size=e.get_object_field(src,"mTextSize")?.as_float();
    flags=e.get_object_field(src,"mFlags")?.as_int();
   }else if a.len()>1{flags=int(a,1,0);}
   for (k,v) in [
    ("mColor",Value::Int(color as i32)),("mAlpha",Value::Int((color>>24) as i32)),
    ("mStrokeWidth",Value::Float(width)),("mTextSize",Value::Float(size)),
    ("mFlags",Value::Int(flags)),("mAntiAlias",flag(flags&1!=0)),
    ("mDither",flag(flags&4!=0)),("mFilterBitmap",flag(flags&2!=0)),
    ("mStyle",Value::Int(0)),("mStrokeMiter",Value::Float(4.0)),
    ("mFakeBoldText",Value::Int(0)),("mUnderlineText",Value::Int(0)),
    ("mStrikeThruText",Value::Int(0)),("mSubpixelText",Value::Int(0)),
    ("mLinearText",Value::Int(0)),("mTypeface",Value::Null),
    ("mStyleObject",Value::Null),("mTextAlignObject",Value::Null),
    ("mStrokeCapObject",Value::Null),("mStrokeJoinObject",Value::Null)
   ]{e.set_object_field(p,k,v)?;}
   Ok(None)
  });
 }
 j.register("Landroid/graphics/Paint;","setColor","(I)V",|e,a|{let p=id(a)?;let c=int(a,1,0xff000000u32 as i32);e.set_object_field(p,"mColor",Value::Int(c))?;e.set_object_field(p,"mAlpha",Value::Int(((c as u32)>>24) as i32))?;Ok(None)});
 j.register("Landroid/graphics/Paint;","getColor","()I",|e,a|Ok(Some(e.get_object_field(id(a)?,"mColor")?)));
 j.register("Landroid/graphics/Paint;","setAlpha","(I)V",|e,a|{let p=id(a)?;let alpha=int(a,1,255).clamp(0,255);let c=e.get_object_field(p,"mColor")?.as_int() as u32;let next=((alpha as u32)<<24)|(c&0xffffff);e.set_object_field(p,"mColor",Value::Int(next as i32))?;e.set_object_field(p,"mAlpha",Value::Int(alpha))?;Ok(None)});
 j.register("Landroid/graphics/Paint;","getAlpha","()I",|e,a|Ok(Some(e.get_object_field(id(a)?,"mAlpha")?)));
 j.register("Landroid/graphics/Paint;","setARGB","(IIII)V",|e,a|{let p=id(a)?;let c=((int(a,1,255).clamp(0,255) as u32)<<24)|((int(a,2,0).clamp(0,255) as u32)<<16)|((int(a,3,0).clamp(0,255) as u32)<<8)|(int(a,4,0).clamp(0,255) as u32);e.set_object_field(p,"mColor",Value::Int(c as i32))?;e.set_object_field(p,"mAlpha",Value::Int((c>>24) as i32))?;Ok(None)});
 for (set,get,field) in [("setStrokeWidth","getStrokeWidth","mStrokeWidth"),("setTextSize","getTextSize","mTextSize"),("setStrokeMiter","getStrokeMiter","mStrokeMiter")]{
  j.register("Landroid/graphics/Paint;",set,"(F)V",move|e,a|{let mut v=flt(a,1,0.0);if field=="mStrokeWidth"{v=v.max(0.0);}e.set_object_field(id(a)?,field,Value::Float(v))?;Ok(None)});
  j.register("Landroid/graphics/Paint;",get,"()F",move|e,a|Ok(Some(e.get_object_field(id(a)?,field)?)));
 }
 for (set,get,field) in [("setAntiAlias","isAntiAlias","mAntiAlias"),("setDither","isDither","mDither"),("setFilterBitmap","isFilterBitmap","mFilterBitmap"),("setFakeBoldText","isFakeBoldText","mFakeBoldText"),("setUnderlineText","isUnderlineText","mUnderlineText"),("setStrikeThruText","isStrikeThruText","mStrikeThruText"),("setSubpixelText","isSubpixelText","mSubpixelText"),("setLinearText","isLinearText","mLinearText")]{
  j.register("Landroid/graphics/Paint;",set,"(Z)V",move|e,a|{e.set_object_field(id(a)?,field,flag(int(a,1,0)!=0))?;Ok(None)});
  j.register("Landroid/graphics/Paint;",get,"()Z",move|e,a|Ok(Some(e.get_object_field(id(a)?,field)?)));
 }
 j.register("Landroid/graphics/Paint;","setFlags","(I)V",|e,a|{let p=id(a)?;let f=int(a,1,0);e.set_object_field(p,"mFlags",Value::Int(f))?;e.set_object_field(p,"mAntiAlias",flag(f&1!=0))?;e.set_object_field(p,"mFilterBitmap",flag(f&2!=0))?;e.set_object_field(p,"mDither",flag(f&4!=0))?;Ok(None)});
 j.register("Landroid/graphics/Paint;","getFlags","()I",|e,a|Ok(Some(e.get_object_field(id(a)?,"mFlags")?)));
 for (set,get,field,proto,ret) in [
  ("setStyle","getStyle","mStyleObject","(Landroid/graphics/Paint$Style;)V","()Landroid/graphics/Paint$Style;"),
  ("setTypeface","getTypeface","mTypeface","(Landroid/graphics/Typeface;)V","()Landroid/graphics/Typeface;"),
  ("setTextAlign","getTextAlign","mTextAlignObject","(Landroid/graphics/Paint$Align;)V","()Landroid/graphics/Paint$Align;"),
  ("setStrokeCap","getStrokeCap","mStrokeCapObject","(Landroid/graphics/Paint$Cap;)V","()Landroid/graphics/Paint$Cap;"),
  ("setStrokeJoin","getStrokeJoin","mStrokeJoinObject","(Landroid/graphics/Paint$Join;)V","()Landroid/graphics/Paint$Join;")
 ]{
  j.register("Landroid/graphics/Paint;",set,proto,move|e,a|{e.set_object_field(id(a)?,field,a.get(1).copied().unwrap_or(Value::Null))?;Ok(None)});
  j.register("Landroid/graphics/Paint;",get,ret,move|e,a|Ok(Some(e.get_object_field(id(a)?,field)?)));
 }
 j.register("Landroid/graphics/Paint;","measureText","(Ljava/lang/String;)F",|e,a|{let p=id(a)?;let text=a.get(1).and_then(Value::as_object).ok_or("measureText text is null")?;let s=e.get_string_utf(text)?;let size=e.get_object_field(p,"mTextSize")?.as_float();Ok(Some(Value::Float(s.chars().count() as f32*size*0.5)))});
}
#[cfg(test)] mod tests{
 use super::*;use crate::heap::ManagedHeap;use crate::jni::JniEnv;
 fn call(r:&JniNativeRegistry,h:&mut ManagedHeap,n:&str,p:&str,a:&[Value])->Option<Value>{let f=r.find("Landroid/graphics/Paint;",n,p).unwrap().clone();f(&mut JniEnv::new(h),a).unwrap()}
 #[test]fn constructor_and_color_roundtrip(){let mut r=JniNativeRegistry::new();register(&mut r);let mut h=ManagedHeap::default();let p=h.allocate_instance("Landroid/graphics/Paint;").unwrap();call(&r,&mut h,"<init>","()V",&[Value::Object(p)]);assert_eq!(h.get_instance_field(p,"mColor").unwrap(),Value::Int(0xff000000u32 as i32));call(&r,&mut h,"setColor","(I)V",&[Value::Object(p),Value::Int(0xff123456u32 as i32)]);call(&r,&mut h,"setAlpha","(I)V",&[Value::Object(p),Value::Int(128)]);assert_eq!(call(&r,&mut h,"getColor","()I",&[Value::Object(p)]),Some(Value::Int(0x80123456u32 as i32)));}
 #[test]fn flags_text_size_and_measurement(){let mut r=JniNativeRegistry::new();register(&mut r);let mut h=ManagedHeap::default();let p=h.allocate_instance("Landroid/graphics/Paint;").unwrap();let t=h.allocate_string("Stride").unwrap();call(&r,&mut h,"<init>","()V",&[Value::Object(p)]);call(&r,&mut h,"setTextSize","(F)V",&[Value::Object(p),Value::Float(20.0)]);call(&r,&mut h,"setAntiAlias","(Z)V",&[Value::Object(p),Value::Int(0)]);assert_eq!(call(&r,&mut h,"isAntiAlias","()Z",&[Value::Object(p)]),Some(Value::Int(0)));assert_eq!(call(&r,&mut h,"measureText","(Ljava/lang/String;)F",&[Value::Object(p),Value::Object(t)]),Some(Value::Float(60.0)));}
}
pub(crate) fn register_core_paint_methods(j:&mut JniNativeRegistry){register(j);}
