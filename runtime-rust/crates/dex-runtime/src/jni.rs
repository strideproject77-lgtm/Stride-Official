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
}

impl<'a> JniEnv<'a> {
    pub fn new(heap: &'a mut ManagedHeap) -> Self {
        Self {
            heap,
            local_refs: Vec::new(),
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
}

/// JNI native method registry for binding native library symbols to DEX method declarations.
#[derive(Default, Clone)]
pub struct JniNativeRegistry {
    methods: HashMap<NativeMethodKey, NativeHandler>,
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

    pub fn find(&self, class: &str, name: &str, prototype: &str) -> Option<&NativeHandler> {
        self.methods.get(&NativeMethodKey {
            class_descriptor: class.to_owned(),
            name: name.to_owned(),
            prototype: prototype.to_owned(),
        })
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

        // android.util.Log.i(String, String)I
        self.register(
            "Landroid/util/Log;",
            "i",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            |_env, _args| {
                Ok(Some(Value::Int(1)))
            },
        );
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
    }
}
