//! Managed SharedPreferences JNI bridge. Values are retained per runtime session.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::jni::JniNativeRegistry;
use crate::types::Value;

pub type PreferenceStore = Arc<Mutex<HashMap<String, HashMap<String, Value>>>>;

fn string_arg(env: &crate::jni::JniEnv<'_>, args: &[Value], index: usize) -> Result<String, String> {
    let id = args.get(index).and_then(Value::as_object).ok_or_else(|| format!("SharedPreferences string argument {index} is null"))?;
    Ok(env.get_string_utf(id)?.to_owned())
}
fn object(args: &[Value], index: usize) -> Result<u32, String> {
    args.get(index).and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| format!("SharedPreferences receiver argument {index} is null"))
}
fn register(jni: &mut JniNativeRegistry, store: PreferenceStore) {
    let shared = store.clone();
    jni.register("Landroid/app/Activity;", "getSharedPreferences",
        "(Ljava/lang/String;I)Landroid/content/SharedPreferences;", move |env, args| {
            let activity = object(args, 0)?;
            let name = string_arg(env, args, 1)?;
            let prefs = env.heap.allocate_instance("Landroid/content/SharedPreferences;")
                .map_err(|error| format!("SharedPreferences allocation failed: {error:?}"))?;
            let name_id = env.heap.allocate_string(&name)
                .map_err(|error| format!("SharedPreferences name allocation failed: {error:?}"))?;
            env.set_object_field(prefs, "mStrideName", Value::Object(name_id))?;
            env.set_object_field(prefs, "mStrideActivity", Value::Object(activity))?;
            shared.lock().map_err(|_| "SharedPreferences store lock poisoned")?
                .entry(name).or_default();
            Ok(Some(Value::Object(prefs)))
        });
    jni.register("Landroid/content/SharedPreferences;", "edit",
        "()Landroid/content/SharedPreferences$Editor;", |env, args| {
            let prefs = object(args, 0)?;
            let editor = env.heap.allocate_instance("Landroid/content/SharedPreferences$Editor;")
                .map_err(|error| format!("SharedPreferences.Editor allocation failed: {error:?}"))?;
            let name = env.get_object_field(prefs, "mStrideName")?;
            env.set_object_field(editor, "mStrideName", name)?;
            Ok(Some(Value::Object(editor)))
        });
    for (name, prototype, default_kind) in [
        ("getBoolean", "(Ljava/lang/String;Z)Z", 0),
        ("getInt", "(Ljava/lang/String;I)I", 1),
        ("getLong", "(Ljava/lang/String;J)J", 2),
    ] {
        let store = store.clone();
        jni.register("Landroid/content/SharedPreferences;", name, prototype, move |env, args| {
            let prefs = object(args, 0)?;
            let file = env.get_object_field(prefs, "mStrideName")?.as_object()
                .ok_or("SharedPreferences name is missing")?;
            let file = env.get_string_utf(file)?.to_owned();
            let key = string_arg(env, args, 1)?;
            let fallback = args.get(2).copied().unwrap_or(match default_kind {
                0 | 1 => Value::Int(0), _ => Value::Long(0),
            });
            let guard = store.lock().map_err(|_| "SharedPreferences store lock poisoned")?;
            let value = guard.get(&file).and_then(|entries| entries.get(&key)).copied().unwrap_or(fallback);
            Ok(Some(value))
        });
    }
    jni.register("Landroid/content/SharedPreferences;", "contains", "(Ljava/lang/String;)Z", {
        let store = store.clone();
        move |env, args| {
            let prefs = object(args, 0)?;
            let file = env.get_object_field(prefs, "mStrideName")?.as_object().ok_or("SharedPreferences name is missing")?;
            let file = env.get_string_utf(file)?.to_owned();
            let key = string_arg(env, args, 1)?;
            let present = store.lock().map_err(|_| "SharedPreferences store lock poisoned")?
                .get(&file).is_some_and(|entries| entries.contains_key(&key));
            Ok(Some(Value::Int(if present { 1 } else { 0 })))
        }
    });
    for (name, prototype, value_index) in [
        ("putBoolean", "(Ljava/lang/String;Z)Landroid/content/SharedPreferences$Editor;", 2usize),
        ("putInt", "(Ljava/lang/String;I)Landroid/content/SharedPreferences$Editor;", 2usize),
        ("putLong", "(Ljava/lang/String;J)Landroid/content/SharedPreferences$Editor;", 2usize),
    ] {
        let store = store.clone();
        jni.register("Landroid/content/SharedPreferences$Editor;", name, prototype, move |env, args| {
            let editor = object(args, 0)?;
            let file = env.get_object_field(editor, "mStrideName")?.as_object().ok_or("Editor preference name is missing")?;
            let file = env.get_string_utf(file)?.to_owned();
            let key = string_arg(env, args, 1)?;
            let value = args.get(value_index).copied().unwrap_or(Value::Int(0));
            store.lock().map_err(|_| "SharedPreferences store lock poisoned")?
                .entry(file).or_default().insert(key, value);
            Ok(Some(Value::Object(editor)))
        });
    }
    jni.register("Landroid/content/SharedPreferences$Editor;", "remove",
        "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;", {
            let store = store.clone();
            move |env, args| {
                let editor = object(args, 0)?;
                let file = env.get_object_field(editor, "mStrideName")?.as_object().ok_or("Editor preference name is missing")?;
                let file = env.get_string_utf(file)?.to_owned();
                let key = string_arg(env, args, 1)?;
                if let Some(entries) = store.lock().map_err(|_| "SharedPreferences store lock poisoned")?.get_mut(&file) {
                    entries.remove(&key);
                }
                Ok(Some(Value::Object(editor)))
            }
        });
    for (name, prototype, returns_bool) in [("apply", "()V", false), ("commit", "()Z", true)] {
        jni.register("Landroid/content/SharedPreferences$Editor;", name, prototype, move |_env, args| {
            let editor = object(args, 0)?;
            Ok(if returns_bool { Some(Value::Int(1)) } else { None })
        });
    }
}
pub(crate) fn register_shared_preferences(jni: &mut JniNativeRegistry, store: PreferenceStore) {
    register(jni, store);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;
    #[test]
    fn preference_values_round_trip_through_editor() {
        let store = Arc::new(Mutex::new(HashMap::new()));
        let mut registry = JniNativeRegistry::new();
        register(&mut registry, store);
        let mut heap = ManagedHeap::default();
        let activity = heap.allocate_instance("Lgame/MainActivity;").unwrap();
        let name = heap.allocate_string("game_state").unwrap();
        let key = heap.allocate_string("score").unwrap();
        let get = registry.find("Landroid/app/Activity;", "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;").unwrap().clone();
        let prefs = get(&mut JniEnv::new(&mut heap), &[Value::Object(activity), Value::Object(name), Value::Int(0)])
            .unwrap().unwrap().as_object().unwrap();
        let edit = registry.find("Landroid/content/SharedPreferences;", "edit",
            "()Landroid/content/SharedPreferences$Editor;").unwrap().clone();
        let editor = edit(&mut JniEnv::new(&mut heap), &[Value::Object(prefs)]).unwrap().unwrap().as_object().unwrap();
        let put = registry.find("Landroid/content/SharedPreferences$Editor;", "putInt",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences$Editor;").unwrap().clone();
        put(&mut JniEnv::new(&mut heap), &[Value::Object(editor), Value::Object(key), Value::Int(2048)]).unwrap();
        let read = registry.find("Landroid/content/SharedPreferences;", "getInt", "(Ljava/lang/String;I)I").unwrap().clone();
        assert_eq!(read(&mut JniEnv::new(&mut heap), &[Value::Object(prefs), Value::Object(key), Value::Int(0)]).unwrap(), Some(Value::Int(2048)));
    }
}
