//! Typeface compatibility metadata. Actual font rasterization is delegated to
//! the host text backend; unsupported packaged fonts retain a diagnostic identity.
use crate::jni::JniNativeRegistry;
use crate::types::Value;

fn register(jni: &mut JniNativeRegistry) {
    jni.register("Landroid/graphics/Typeface;", "createFromAsset",
        "(Landroid/content/res/AssetManager;Ljava/lang/String;)Landroid/graphics/Typeface;", |env, args| {
            let name_id = args.get(1).and_then(Value::as_object).ok_or("Typeface asset manager is null")?;
            let path_id = args.get(2).and_then(Value::as_object).ok_or("Typeface asset path is null")?;
            let path = env.get_string_utf(path_id)?.to_owned();
            let face = env.heap.allocate_instance("Landroid/graphics/Typeface;")
                .map_err(|error| format!("Typeface allocation failed: {error:?}"))?;
            env.set_object_field(face, "mStrideAssetPath", Value::Object(env.heap.allocate_string(&path)
                .map_err(|error| format!("Typeface path allocation failed: {error:?}"))?))?;
            env.set_object_field(face, "mStrideAssetManager", Value::Object(name_id))?;
            env.set_object_field(face, "mStrideStyle", Value::Int(0))?;
            Ok(Some(Value::Object(face)))
        });
    jni.register("Landroid/graphics/Typeface;", "create", "(Ljava/lang/String;I)Landroid/graphics/Typeface;", |env, args| {
        let family = args.get(0).and_then(Value::as_object).unwrap_or(0);
        let style = args.get(1).map(Value::as_int).unwrap_or(0);
        let face = env.heap.allocate_instance("Landroid/graphics/Typeface;")
            .map_err(|error| format!("Typeface allocation failed: {error:?}"))?;
        env.set_object_field(face, "mStrideAssetPath", Value::Object(family))?;
        env.set_object_field(face, "mStrideStyle", Value::Int(style))?;
        Ok(Some(Value::Object(face)))
    });
    jni.register("Landroid/graphics/Typeface;", "getStyle", "()I", |env, args| {
        let face = args.first().and_then(Value::as_object).ok_or("Typeface receiver is null")?;
        Ok(Some(env.get_object_field(face, "mStrideStyle")?))
    });
    for (name, mask) in [("isBold", 1), ("isItalic", 2)] {
        jni.register("Landroid/graphics/Typeface;", name, "()Z", move |env, args| {
            let face = args.first().and_then(Value::as_object).ok_or("Typeface receiver is null")?;
            let style = env.get_object_field(face, "mStrideStyle")?.as_int();
            Ok(Some(Value::Int(if style & mask != 0 { 1 } else { 0 })))
        });
    }
}
pub(crate) fn register_core_typeface_methods(jni: &mut JniNativeRegistry) { register(jni); }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;
    #[test]
    fn asset_typeface_retains_requested_asset_name() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let manager = heap.allocate_instance("Landroid/content/res/AssetManager;").unwrap();
        let path = heap.allocate_string("ClearSans-Bold.ttf").unwrap();
        let create = registry.find("Landroid/graphics/Typeface;", "createFromAsset",
            "(Landroid/content/res/AssetManager;Ljava/lang/String;)Landroid/graphics/Typeface;").unwrap().clone();
        let face = create(&mut JniEnv::new(&mut heap), &[Value::Object(manager), Value::Object(path)])
            .unwrap().unwrap().as_object().unwrap();
        let stored = heap.get_instance_field(face, "mStrideAssetPath").unwrap().as_object().unwrap();
        assert_eq!(heap.get_string(stored).unwrap(), "ClearSans-Bold.ttf");
    }
}
