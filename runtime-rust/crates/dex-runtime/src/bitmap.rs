//! Managed android.graphics.Bitmap compatibility for software-rendered 2D apps.
use crate::jni::JniNativeRegistry;
use crate::types::Value;

fn object(args: &[Value], index: usize) -> Result<u32, String> {
    args.get(index).and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| format!("Bitmap argument {index} is null or invalid"))
}
fn int(args: &[Value], index: usize, default: i32) -> i32 {
    args.get(index).map(Value::as_int).unwrap_or(default)
}
fn dimensions(heap: &crate::heap::ManagedHeap, bitmap: u32) -> Result<(i32, i32, u32), String> {
    let width = heap.get_instance_field(bitmap, "mStrideWidth")?.as_int();
    let height = heap.get_instance_field(bitmap, "mStrideHeight")?.as_int();
    let pixels = heap.get_instance_field(bitmap, "mStridePixels")?.as_object()
        .ok_or("Bitmap pixel buffer is missing")?;
    Ok((width, height, pixels))
}
fn register(jni: &mut JniNativeRegistry) {
    jni.register("Landroid/graphics/Bitmap;", "createBitmap",
        "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;", |env, args| {
            let width = int(args, 0, 0);
            let height = int(args, 1, 0);
            if width <= 0 || height <= 0 || width > 8192 || height > 8192
                || (width as i64) * (height as i64) > 16_777_216 {
                return Err(format!("Bitmap dimensions are invalid or exceed safety limits: {width}x{height}"));
            }
            let bitmap = env.heap.allocate_instance("Landroid/graphics/Bitmap;")
                .map_err(|error| format!("Bitmap allocation failed: {error:?}"))?;
            let pixels = env.heap.allocate_array("[I", width * height)
                .map_err(|error| format!("Bitmap pixel allocation failed: {error:?}"))?;
            env.set_object_field(bitmap, "mStrideWidth", Value::Int(width))?;
            env.set_object_field(bitmap, "mStrideHeight", Value::Int(height))?;
            env.set_object_field(bitmap, "mStridePixels", Value::Object(pixels))?;
            env.set_object_field(bitmap, "mStrideHasAlpha", Value::Int(1))?;
            Ok(Some(Value::Object(bitmap)))
        });
    for (name, field) in [("getWidth", "mStrideWidth"), ("getHeight", "mStrideHeight")] {
        jni.register("Landroid/graphics/Bitmap;", name, "()I", move |env, args| {
            let bitmap = object(args, 0)?;
            Ok(Some(env.get_object_field(bitmap, field)?))
        });
    }
    jni.register("Landroid/graphics/Bitmap;", "setPixel", "(III)V", |env, args| {
        let bitmap = object(args, 0)?;
        let x = int(args, 1, -1);
        let y = int(args, 2, -1);
        let color = int(args, 3, 0);
        let (width, height, pixels) = dimensions(env.heap, bitmap)?;
        if x < 0 || y < 0 || x >= width || y >= height { return Err("Bitmap.setPixel coordinates are out of bounds".into()); }
        env.heap.set_array_element(pixels, y * width + x, Value::Int(color))
            .map_err(|error| format!("Bitmap.setPixel failed: {error:?}"))?;
        Ok(None)
    });
    jni.register("Landroid/graphics/Bitmap;", "getPixel", "(II)I", |env, args| {
        let bitmap = object(args, 0)?;
        let x = int(args, 1, -1);
        let y = int(args, 2, -1);
        let (width, height, pixels) = dimensions(env.heap, bitmap)?;
        if x < 0 || y < 0 || x >= width || y >= height { return Err("Bitmap.getPixel coordinates are out of bounds".into()); }
        Ok(Some(env.heap.get_array_element(pixels, y * width + x)
            .map_err(|error| format!("Bitmap.getPixel failed: {error:?}"))?))
    });
    jni.register("Landroid/graphics/Bitmap;", "eraseColor", "(I)V", |env, args| {
        let bitmap = object(args, 0)?;
        let color = int(args, 1, 0);
        let (width, height, pixels) = dimensions(env.heap, bitmap)?;
        for index in 0..(width * height) {
            env.heap.set_array_element(pixels, index, Value::Int(color))
                .map_err(|error| format!("Bitmap.eraseColor failed: {error:?}"))?;
        }
        Ok(None)
    });
    jni.register("Landroid/graphics/Bitmap;", "hasAlpha", "()Z", |env, args| {
        let bitmap = object(args, 0)?;
        Ok(Some(env.get_object_field(bitmap, "mStrideHasAlpha")?))
    });
    jni.register("Landroid/graphics/Bitmap;", "setHasAlpha", "(Z)V", |env, args| {
        let bitmap = object(args, 0)?;
        env.set_object_field(bitmap, "mStrideHasAlpha", Value::Int(if int(args, 1, 0) != 0 { 1 } else { 0 }))?;
        Ok(None)
    });
}
pub(crate) fn register_core_bitmap_methods(jni: &mut JniNativeRegistry) { register(jni); }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;

    #[test]
    fn bitmap_dimensions_and_pixel_round_trip() {
        let mut jni = JniNativeRegistry::new();
        register(&mut jni);
        let mut heap = ManagedHeap::default();
        let config = heap.allocate_instance("Landroid/graphics/Bitmap$Config;").unwrap();
        let create = jni.find("Landroid/graphics/Bitmap;", "createBitmap",
            "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;").unwrap().clone();
        let bitmap = create(&mut JniEnv::new(&mut heap), &[Value::Int(3), Value::Int(2), Value::Object(config)])
            .unwrap().unwrap().as_object().unwrap();
        let set = jni.find("Landroid/graphics/Bitmap;", "setPixel", "(III)V").unwrap().clone();
        set(&mut JniEnv::new(&mut heap), &[Value::Object(bitmap), Value::Int(1), Value::Int(1), Value::Int(0xff123456u32 as i32)]).unwrap();
        let get = jni.find("Landroid/graphics/Bitmap;", "getPixel", "(II)I").unwrap().clone();
        assert_eq!(get(&mut JniEnv::new(&mut heap), &[Value::Object(bitmap), Value::Int(1), Value::Int(1)]).unwrap(),
            Some(Value::Int(0xff123456u32 as i32)));
    }
}
