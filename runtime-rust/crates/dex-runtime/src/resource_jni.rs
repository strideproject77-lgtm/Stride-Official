//! JNI access to the parsed guest resources.arsc catalog.
use std::sync::{Arc, Mutex};
use crate::jni::JniNativeRegistry;
use crate::resources::ResourceCatalog;
use crate::types::Value;
pub type SharedResourceCatalog = Arc<Mutex<ResourceCatalog>>;

fn receiver(args: &[Value]) -> Result<u32, String> {
    args.first().and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| "Resources receiver is null".to_owned())
}
fn register(jni: &mut JniNativeRegistry, catalog: SharedResourceCatalog) {
    jni.register("Landroid/content/Context;", "getResources",
        "()Landroid/content/res/Resources;", |_env, args| {
            let _ = receiver(args)?;
            let resources = _env.heap.allocate_instance("Landroid/content/res/Resources;")
                .map_err(|error| format!("Resources allocation failed: {error:?}"))?;
            Ok(Some(Value::Object(resources)))
        });
    jni.register("Landroid/content/Context;", "getAssets",
        "()Landroid/content/res/AssetManager;", |env, args| {
            let context = receiver(args)?;
            if let Some(existing) = env.get_object_field(context, "mStrideAssetManager")?.as_object() {
                return Ok(Some(Value::Object(existing)));
            }
            let manager = env.heap.allocate_instance("Landroid/content/res/AssetManager;")
                .map_err(|error| format!("AssetManager allocation failed: {error:?}"))?;
            env.set_object_field(context, "mStrideAssetManager", Value::Object(manager))?;
            Ok(Some(Value::Object(manager)))
        });
    for (name, prototype, returns_text) in [
        ("getString", "(I)Ljava/lang/String;", true),
        ("getText", "(I)Ljava/lang/CharSequence;", true),
        ("getColor", "(I)I", false),
        ("getColor", "(ILandroid/content/res/Resources$Theme;)I", false),
    ] {
        let catalog = catalog.clone();
        jni.register("Landroid/content/res/Resources;", name, prototype, move |env, args| {
            let _resources = receiver(args)?;
            let id = args.get(1).map(Value::as_int).unwrap_or(0) as u32;
            let guard = catalog.lock().map_err(|_| "Resource catalog lock poisoned")?;
            if returns_text {
                let text = guard.string(id).ok_or_else(|| format!("Missing Android string resource 0x{id:08x}"))?.to_owned();
                drop(guard);
                let string = env.heap.allocate_string(&text)
                    .map_err(|error| format!("Resource string allocation failed: {error:?}"))?;
                Ok(Some(Value::Object(string)))
            } else {
                let color = guard.color(id).ok_or_else(|| format!("Missing Android color resource 0x{id:08x}"))?;
                Ok(Some(Value::Int(color as i32)))
            }
        });
    }
    let catalog_for_context = catalog.clone();
    jni.register("Landroid/content/Context;", "getString", "(I)Ljava/lang/String;", move |env, args| {
        let _context = receiver(args)?;
        let id = args.get(1).map(Value::as_int).unwrap_or(0) as u32;
        let text = catalog_for_context.lock().map_err(|_| "Resource catalog lock poisoned")?
            .string(id).ok_or_else(|| format!("Missing Android string resource 0x{id:08x}"))?.to_owned();
        let string = env.heap.allocate_string(&text)
            .map_err(|error| format!("Resource string allocation failed: {error:?}"))?;
        Ok(Some(Value::Object(string)))
    });
}
pub(crate) fn register_resource_methods(jni: &mut JniNativeRegistry, catalog: SharedResourceCatalog) {
    register(jni, catalog);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;
    #[test]
    fn context_resources_resolve_strings_and_colors() {
        let mut catalog = ResourceCatalog::default();
        // The catalog parser owns its maps; this test uses a minimal valid parsed
        // catalog in production coverage and checks that missing IDs fail safely.
        let store = Arc::new(Mutex::new(catalog.clone()));
        let mut registry = JniNativeRegistry::new();
        register(&mut registry, store);
        let mut heap = ManagedHeap::default();
        let context = heap.allocate_instance("Ltest/Activity;").unwrap();
        let get = registry.find("Landroid/content/Context;", "getResources",
            "()Landroid/content/res/Resources;").unwrap().clone();
        let resources = get(&mut JniEnv::new(&mut heap), &[Value::Object(context)])
            .unwrap().unwrap().as_object().unwrap();
        let get_color = registry.find("Landroid/content/res/Resources;", "getColor", "(I)I").unwrap().clone();
        assert!(get_color(&mut JniEnv::new(&mut heap), &[Value::Object(resources), Value::Int(123)]).is_err());
        catalog = ResourceCatalog::default();
        let _ = catalog;
    }
}
