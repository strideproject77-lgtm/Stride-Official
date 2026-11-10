//! Drawable/BitmapDrawable state bridge used by Canvas-based games.
use crate::jni::{JniEnv, JniNativeRegistry};
use crate::types::Value;

fn receiver(args: &[Value]) -> Result<u32, String> {
    args.first().and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| "Drawable receiver is null".to_owned())
}
fn register(jni: &mut JniNativeRegistry) {
    for prototype in [
        "(Landroid/content/res/Resources;Landroid/graphics/Bitmap;)V",
        "(Landroid/graphics/Bitmap;)V",
    ] {
        jni.register("Landroid/graphics/drawable/BitmapDrawable;", "<init>", prototype, |env, args| {
            let drawable = receiver(args)?;
            let bitmap_index = if args.len() >= 3 { 2 } else { 1 };
            let bitmap = args.get(bitmap_index).and_then(Value::as_object)
                .ok_or("BitmapDrawable constructor received a null Bitmap")?;
            env.set_object_field(drawable, "mStrideBitmap", Value::Object(bitmap))?;
            env.set_object_field(drawable, "mStrideAlpha", Value::Int(255))?;
            env.set_object_field(drawable, "mStrideLeft", Value::Int(0))?;
            env.set_object_field(drawable, "mStrideTop", Value::Int(0))?;
            env.set_object_field(drawable, "mStrideRight", Value::Int(0))?;
            env.set_object_field(drawable, "mStrideBottom", Value::Int(0))?;
            Ok(None)
        });
    }
    for class in ["Landroid/graphics/drawable/Drawable;", "Landroid/graphics/drawable/BitmapDrawable;"] {
        jni.register(class, "setAlpha", "(I)V", |env, args| {
            let drawable = receiver(args)?;
            let alpha = args.get(1).map(Value::as_int).unwrap_or(255).clamp(0, 255);
            env.set_object_field(drawable, "mStrideAlpha", Value::Int(alpha))?;
            Ok(None)
        });
        jni.register(class, "setBounds", "(IIII)V", |env, args| {
            let drawable = receiver(args)?;
            for (field, index) in [
                ("mStrideLeft", 1), ("mStrideTop", 2),
                ("mStrideRight", 3), ("mStrideBottom", 4),
            ] {
                env.set_object_field(drawable, field, Value::Int(args.get(index).map(Value::as_int).unwrap_or(0)))?;
            }
            Ok(None)
        });
        jni.register(class, "draw", "(Landroid/graphics/Canvas;)V", |env, args| {
            let drawable = receiver(args)?;
            let canvas = args.get(1).and_then(Value::as_object).ok_or("Drawable.draw Canvas is null")?;
            let bitmap = env.get_object_field(drawable, "mStrideBitmap")?.as_object()
                .ok_or("BitmapDrawable has no Bitmap")?;
            let left = env.get_object_field(drawable, "mStrideLeft")?.as_int() as f32;
            let top = env.get_object_field(drawable, "mStrideTop")?.as_int() as f32;
            crate::canvas_jni::record_bitmap_draw(env, canvas, bitmap, left, top)?;
            Ok(None)
        });
    }
}
pub(crate) fn register_core_drawable_methods(jni: &mut JniNativeRegistry) { register(jni); }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;
    #[test]
    fn bitmap_drawable_preserves_bitmap_and_bounds() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let drawable = heap.allocate_instance("Landroid/graphics/drawable/BitmapDrawable;").unwrap();
        let bitmap = heap.allocate_instance("Landroid/graphics/Bitmap;").unwrap();
        let ctor = registry.find("Landroid/graphics/drawable/BitmapDrawable;", "<init>",
            "(Landroid/graphics/Bitmap;)V").unwrap().clone();
        ctor(&mut JniEnv::new(&mut heap), &[Value::Object(drawable), Value::Object(bitmap)]).unwrap();
        let bounds = registry.find("Landroid/graphics/drawable/BitmapDrawable;", "setBounds", "(IIII)V").unwrap().clone();
        bounds(&mut JniEnv::new(&mut heap), &[Value::Object(drawable), Value::Int(4), Value::Int(5), Value::Int(40), Value::Int(50)]).unwrap();
        assert_eq!(heap.get_instance_field(drawable, "mStrideBitmap").unwrap(), Value::Object(bitmap));
        assert_eq!(heap.get_instance_field(drawable, "mStrideLeft").unwrap(), Value::Int(4));
        assert_eq!(heap.get_instance_field(drawable, "mStrideBottom").unwrap(), Value::Int(50));
    }
}
