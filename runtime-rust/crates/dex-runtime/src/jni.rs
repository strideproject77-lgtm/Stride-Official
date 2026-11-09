//! JNI native compatibility bridge and method dispatch boundary.

use std::collections::HashMap;
use std::sync::Arc;
use crate::types::Value;
use crate::heap::ManagedHeap;

pub type NativeHandler = Arc<dyn Fn(&mut JniEnv, &[Value]) -> Result<Option<Value>, String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NativeMethodKey {
    pub class_descriptor: String,
    pub name: String,
    pub prototype: String,
}

/// JNI native interface environment providing memory and object operations to native code.
pub struct JniEnv<'a> {
    pub heap: &'a mut ManagedHeap,
    pub local_refs: Vec<u32>,
    pub local_frames: Vec<usize>,
}

impl<'a> JniEnv<'a> {
    pub fn new(heap: &'a mut ManagedHeap) -> Self {
        Self {
            heap,
            local_refs: Vec::new(),
            local_frames: Vec::new(),
        }
    }

    pub fn push_local_frame(&mut self, _capacity: usize) {
        self.local_frames.push(self.local_refs.len());
    }

    pub fn pop_local_frame(&mut self) {
        if let Some(prev_len) = self.local_frames.pop() {
            self.local_refs.truncate(prev_len);
        }
    }

    pub fn new_string_utf(&mut self, text: &str) -> Result<u32, String> {
        let id = self.heap.allocate_string(text).map_err(|e| format!("{:?}", e))?;
        self.local_refs.push(id);
        Ok(id)
    }

    pub fn get_string_utf(&self, string_id: u32) -> Result<&str, String> {
        self.heap.get_string(string_id).map_err(|e| format!("{:?}", e))
    }

    pub fn get_array_length(&self, array_id: u32) -> Result<usize, String> {
        self.heap.array_length(array_id).map_err(|e| format!("{:?}", e))
    }

    pub fn new_int_array(&mut self, length: i32) -> Result<u32, String> {
        let id = self.heap.allocate_array("[I", length).map_err(|e| format!("{:?}", e))?;
        self.local_refs.push(id);
        Ok(id)
    }

    pub fn get_int_array_elements(&self, array_id: u32) -> Result<Vec<i32>, String> {
        let len = self.heap.array_length(array_id).map_err(|e| format!("{:?}", e))?;
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let val = self.heap.get_array_element(array_id, i as i32).map_err(|e| format!("{:?}", e))?;
            out.push(val.as_int());
        }
        Ok(out)
    }

    pub fn new_byte_array(&mut self, length: i32) -> Result<u32, String> {
        let id = self.heap.allocate_array("[B", length).map_err(|e| format!("{:?}", e))?;
        self.local_refs.push(id);
        Ok(id)
    }

    pub fn get_byte_array_elements(&self, array_id: u32) -> Result<Vec<u8>, String> {
        self.heap.get_byte_array(array_id).map_err(|e| format!("{:?}", e))
    }

    pub fn set_byte_array_region(&mut self, array_id: u32, start: i32, bytes: &[u8]) -> Result<(), String> {
        for (i, &b) in bytes.iter().enumerate() {
            self.heap.set_array_element(array_id, start + i as i32, Value::Int(b as i8 as i32))
                .map_err(|e| format!("{:?}", e))?;
        }
        Ok(())
    }

    pub fn get_object_field(&self, obj_id: u32, field_name: &str) -> Result<Value, String> {
        self.heap.get_instance_field(obj_id, field_name).map_err(|e| format!("{:?}", e))
    }

    pub fn set_object_field(&mut self, obj_id: u32, field_name: &str, val: Value) -> Result<(), String> {
        self.heap.set_instance_field(obj_id, field_name, val).map_err(|e| format!("{:?}", e))
    }
}

/// JNI native method registry for binding native library symbols to DEX method declarations.
#[derive(Default, Clone)]
pub struct JniNativeRegistry {
    methods: HashMap<NativeMethodKey, NativeHandler>,
    mangled_symbols: HashMap<String, NativeHandler>,
}

impl JniNativeRegistry {
    pub fn new() -> Self {
        let mut reg = Self::default();
        reg.register_builtins();
        reg
    }

    pub fn register<F>(
        &mut self,
        class_descriptor: &str,
        name: &str,
        prototype: &str,
        handler: F,
    )
    where
        F: for<'env, 'heap> Fn(&'env mut JniEnv<'heap>, &[Value]) -> Result<Option<Value>, String>
            + Send
            + Sync
            + 'static,
    {
        self.methods.insert(
            NativeMethodKey {
                class_descriptor: class_descriptor.to_owned(),
                name: name.to_owned(),
                prototype: prototype.to_owned(),
            },
            Arc::new(handler),
        );
    }

    /// Registers a native function under its C-mangled symbol name (e.g., `Java_com_example_App_init`).
    pub fn register_symbol<F>(&mut self, mangled_name: &str, handler: F)
    where
        F: for<'env, 'heap> Fn(&'env mut JniEnv<'heap>, &[Value]) -> Result<Option<Value>, String>
            + Send
            + Sync
            + 'static,
    {
        self.mangled_symbols.insert(mangled_name.to_owned(), Arc::new(handler));
    }

    pub fn find(&self, class: &str, name: &str, prototype: &str) -> Option<&NativeHandler> {
        if let Some(h) = self.methods.get(&NativeMethodKey {
            class_descriptor: class.to_owned(),
            name: name.to_owned(),
            prototype: prototype.to_owned(),
        }) {
            return Some(h);
        }

        // Check fallback mangled symbol: Java_package_Class_method
        let clean_class = class.trim_start_matches('L').trim_end_matches(';').replace('/', "_");
        let mangled = format!("Java_{}_{}", clean_class, name);
        self.mangled_symbols.get(&mangled)
    }

    /// Resolve a framework bridge by exact method name/prototype when the
    /// framework superclass is absent from the loaded app hierarchy.
    /// Only a unique Android/Java framework match is accepted.
    pub fn find_unique_framework_signature(&self, name: &str, prototype: &str) -> Option<&NativeHandler> {
        let mut matched: Option<&NativeHandler> = None;
        for (key, handler) in &self.methods {
            let framework_owner = key.class_descriptor.starts_with("Landroid/")
                || key.class_descriptor.starts_with("Ljava/")
                || key.class_descriptor.starts_with("Ljavax/")
                || key.class_descriptor.starts_with("Ldalvik/");
            if framework_owner && key.name == name && key.prototype == prototype {
                if matched.is_some() {
                    return None;
                }
                matched = Some(handler);
            }
        }
        matched
    }

    fn register_builtins(&mut self) {
        // java.lang.System.currentTimeMillis()J
        self.register(
            "Ljava/lang/System;",
            "currentTimeMillis",
            "()J",
            |_env, _args| {
                let ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(Some(Value::Long(ms)))
            },
        );

        // java.lang.System.nanoTime()J
        self.register(
            "Ljava/lang/System;",
            "nanoTime",
            "()J",
            |_env, _args| {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0);
                Ok(Some(Value::Long(nanos)))
            },
        );

        // java.lang.System.arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V
        self.register(
            "Ljava/lang/System;",
            "arraycopy",
            "(Ljava/lang/Object;ILjava/lang/Object;II)V",
            |env, args| {
                if args.len() < 5 {
                    return Err("arraycopy requires 5 arguments".into());
                }
                let src_id = args[0].as_object().ok_or("null src array")?;
                let src_pos = args[1].as_int();
                let dst_id = args[2].as_object().ok_or("null dst array")?;
                let dst_pos = args[3].as_int();
                let length = args[4].as_int();

                env.heap.array_copy(src_id, src_pos, dst_id, dst_pos, length)
                    .map_err(|e| format!("{:?}", e))?;
                Ok(None)
            },
        );

        // Core java.lang.String operations used by ordinary Activity input handlers.
        self.register("Ljava/lang/String;", "equals", "(Ljava/lang/Object;)Z", |env, args| {
            let Some(receiver) = args.first().and_then(Value::as_object) else {
                return Ok(Some(Value::Int(0)));
            };
            let left = env.get_string_utf(receiver)?.to_owned();
            let right = args.get(1).and_then(Value::as_object)
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if left == right { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "length", "()I", |env, args| {
            let receiver = args.first().and_then(Value::as_object).ok_or("String.length receiver is null")?;
            let text = env.get_string_utf(receiver)?;
            Ok(Some(Value::Int(text.encode_utf16().count() as i32)))
        });
        self.register("Ljava/lang/String;", "startsWith", "(Ljava/lang/String;)Z", |env, args| {
            let receiver = args.first().and_then(Value::as_object).ok_or("String.startsWith receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let prefix = args.get(1).and_then(Value::as_object)
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if text.starts_with(prefix) { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "contains", "(Ljava/lang/CharSequence;)Z", |env, args| {
            let receiver = args.first().and_then(Value::as_object).ok_or("String.contains receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let needle = args.get(1).and_then(Value::as_object)
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if text.contains(needle) { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "substring", "(I)Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(Value::as_object).ok_or("String.substring receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let start = args.get(1).map(Value::as_int).unwrap_or(0).max(0) as usize;
            let suffix = text.chars().skip(start).collect::<String>();
            Ok(Some(Value::Object(env.new_string_utf(&suffix)?)))
        });
        self.register("Ljava/lang/String;", "concat", "(Ljava/lang/String;)Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(Value::as_object).ok_or("String.concat receiver is null")?;
            let left = env.get_string_utf(receiver)?.to_owned();
            let right = args.get(1).and_then(Value::as_object)
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("").to_owned();
            Ok(Some(Value::Object(env.new_string_utf(&format!("{}{}", left, right))?)))
        });
        self.register("Ljava/lang/String;", "valueOf", "(D)Ljava/lang/String;", |env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Object(env.new_string_utf(&value.to_string())?)))
        });
        self.register("Ljava/lang/Double;", "parseDouble", "(Ljava/lang/String;)D", |env, args| {
            let value = args.first().and_then(Value::as_object)
                .and_then(|id| env.get_string_utf(id).ok())
                .and_then(|text| text.parse::<f64>().ok()).unwrap_or(0.0);
            Ok(Some(Value::Double(value)))
        });

        // android.os.SystemClock.uptimeMillis()J
        self.register(
            "Landroid/os/SystemClock;",
            "uptimeMillis",
            "()J",
            |_env, _args| {
                let ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(Some(Value::Long(ms)))
            },
        );

        // android.util.Log logging bridges
        let log_fn = |_env: &mut JniEnv, _args: &[Value]| Ok(Some(Value::Int(1)));
        self.register("Landroid/util/Log;", "i", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "d", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "e", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "w", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "v", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);

        // java.lang.Math standard operations
        self.register("Ljava/lang/Math;", "abs", "(I)I", |_env, args| {
            let v = args.first().map(|v| v.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(v.abs())))
        });
        self.register("Ljava/lang/Math;", "abs", "(J)J", |_env, args| {
            let v = args.first().map(|v| v.as_long()).unwrap_or(0);
            Ok(Some(Value::Long(v.abs())))
        });
        self.register("Ljava/lang/Math;", "min", "(II)I", |_env, args| {
            let a = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            let b = args.get(1).map(|v| v.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(a.min(b))))
        });
        self.register("Ljava/lang/Math;", "max", "(II)I", |_env, args| {
            let a = args.get(0).map(|v| v.as_int()).unwrap_or(0);
            let b = args.get(1).map(|v| v.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(a.max(b))))
        });
        self.register("Ljava/lang/Math;", "sqrt", "(D)D", |_env, args| {
            let d = args.first().map(|v| v.as_double()).unwrap_or(0.0);
            Ok(Some(Value::Double(d.sqrt())))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jni_native_registry_invokes_registered_handler() {
        let mut reg = JniNativeRegistry::new();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);

        let handler = reg.find("Ljava/lang/System;", "currentTimeMillis", "()J").unwrap();
        let res = handler(&mut env, &[]).unwrap();
        assert!(matches!(res, Some(Value::Long(ts)) if ts > 0));

        let math_handler = reg.find("Ljava/lang/Math;", "abs", "(I)I").unwrap();
        let math_res = math_handler(&mut env, &[Value::Int(-42)]).unwrap();
        assert_eq!(math_res, Some(Value::Int(42)));
    }

    #[test]
    fn framework_signature_fallback_requires_unique_exact_match() {
        let mut reg = JniNativeRegistry::new();
        reg.register("Landroid/app/Activity;", "getWindow", "()Landroid/view/Window;",
            |_env, _args| Ok(None));
        assert!(reg.find_unique_framework_signature("getWindow", "()Landroid/view/Window;").is_some());
        assert!(reg.find_unique_framework_signature("getWindow", "()V").is_none());

        reg.register("Landroid/view/View;", "getWindow", "()Landroid/view/Window;",
            |_env, _args| Ok(None));
        assert!(reg.find_unique_framework_signature("getWindow", "()Landroid/view/Window;").is_none());
    }

    #[test]
    fn jni_local_frame_management() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);

        env.push_local_frame(16);
        let s = env.new_string_utf("temporary").unwrap();
        assert_eq!(env.local_refs.len(), 1);
        assert_eq!(env.get_string_utf(s).unwrap(), "temporary");

        env.pop_local_frame();
        assert_eq!(env.local_refs.len(), 0);
    }
    #[test]
    fn calculator_string_and_double_core_bridges_execute() {
        let mut registry = JniNativeRegistry::new();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let left = env.new_string_utf("12").unwrap();
        let right = env.new_string_utf("3").unwrap();

        let equals = registry.find(
            "Ljava/lang/String;", "equals", "(Ljava/lang/Object;)Z",
        ).unwrap();
        assert_eq!(equals(&mut env, &[Value::Object(left), Value::Object(left)]).unwrap(), Some(Value::Int(1)));

        let concat = registry.find(
            "Ljava/lang/String;", "concat", "(Ljava/lang/String;)Ljava/lang/String;",
        ).unwrap();
        let combined = concat(&mut env, &[Value::Object(left), Value::Object(right)]).unwrap()
            .and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(combined).unwrap(), "123");

        let parse = registry.find(
            "Ljava/lang/Double;", "parseDouble", "(Ljava/lang/String;)D",
        ).unwrap();
        assert_eq!(parse(&mut env, &[Value::Object(combined)]).unwrap(), Some(Value::Double(123.0)));

        let stringify = registry.find(
            "Ljava/lang/String;", "valueOf", "(D)Ljava/lang/String;",
        ).unwrap();
        let rendered = stringify(&mut env, &[Value::Double(2.5)]).unwrap()
            .and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(rendered).unwrap(), "2.5");
    }

}
