//! Managed Heap, Objects, Arrays, Strings, and Mark-Sweep Garbage Collection.

use std::collections::{HashMap, HashSet};
use crate::types::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeapError {
    NullPointer,
    InvalidHandle(u32),
    ArrayIndexOutOfBounds { index: i32, length: usize },
    NegativeArraySize(i32),
    OutOfMemory { requested: usize, available: usize },
    TypeMismatch { expected: String, actual: String },
    FieldNotFound(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceObject {
    pub class_descriptor: String,
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayObject {
    pub element_descriptor: String,
    pub elements: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringObject {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeapObject {
    Instance(InstanceObject),
    Array(ArrayObject),
    String(StringObject),
}

impl HeapObject {
    pub fn class_descriptor(&self) -> &str {
        match self {
            HeapObject::Instance(inst) => &inst.class_descriptor,
            HeapObject::Array(arr) => &arr.element_descriptor,
            HeapObject::String(_) => "Ljava/lang/String;",
        }
    }

    pub fn approximate_size(&self) -> usize {
        match self {
            HeapObject::Instance(inst) => {
                32 + inst.fields.len() * (24 + 16)
            }
            HeapObject::Array(arr) => {
                32 + arr.elements.len() * 16
            }
            HeapObject::String(str_obj) => {
                32 + str_obj.content.len()
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GcReport {
    pub collections_performed: usize,
    pub objects_before: usize,
    pub objects_after: usize,
    pub bytes_reclaimed: usize,
}

/// A bounded managed heap suitable for iOS memory-constrained environments.
/// Implements root-tracing and mark-sweep garbage collection.
#[derive(Debug, Clone)]
pub struct ManagedHeap {
    objects: HashMap<u32, HeapObject>,
    next_id: u32,
    allocated_bytes: usize,
    max_heap_bytes: usize,
    gc_runs: usize,
    total_reclaimed_bytes: usize,
}

impl Default for ManagedHeap {
    fn default() -> Self {
        Self::new(64 * 1024 * 1024) // 64 MB default heap limit for iOS safety
    }
}

impl ManagedHeap {
    pub fn new(max_heap_bytes: usize) -> Self {
        Self {
            objects: HashMap::new(),
            next_id: 1, // 0 is reserved for NULL
            allocated_bytes: 0,
            max_heap_bytes,
            gc_runs: 0,
            total_reclaimed_bytes: 0,
        }
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }

    pub fn max_heap_bytes(&self) -> usize {
        self.max_heap_bytes
    }

    pub fn gc_runs(&self) -> usize {
        self.gc_runs
    }

    pub fn contains(&self, id: u32) -> bool {
        if id == 0 {
            false
        } else {
            self.objects.contains_key(&id)
        }
    }

    pub fn get(&self, id: u32) -> Result<&HeapObject, HeapError> {
        if id == 0 {
            return Err(HeapError::NullPointer);
        }
        self.objects.get(&id).ok_or(HeapError::InvalidHandle(id))
    }

    pub fn get_mut(&mut self, id: u32) -> Result<&mut HeapObject, HeapError> {
        if id == 0 {
            return Err(HeapError::NullPointer);
        }
        self.objects.get_mut(&id).ok_or(HeapError::InvalidHandle(id))
    }

    fn check_and_record_alloc(&mut self, size: usize) -> Result<(), HeapError> {
        if self.allocated_bytes.saturating_add(size) > self.max_heap_bytes {
            return Err(HeapError::OutOfMemory {
                requested: size,
                available: self.max_heap_bytes.saturating_sub(self.allocated_bytes),
            });
        }
        self.allocated_bytes += size;
        Ok(())
    }

    fn next_handle(&mut self) -> Result<u32, HeapError> {
        let handle = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(HeapError::OutOfMemory {
            requested: 1,
            available: 0,
        })?;
        Ok(handle)
    }

    // --- Instance Allocation and Field Access ---

    pub fn allocate_instance(&mut self, class_descriptor: &str) -> Result<u32, HeapError> {
        let obj = HeapObject::Instance(InstanceObject {
            class_descriptor: class_descriptor.to_owned(),
            fields: HashMap::new(),
        });
        let size = obj.approximate_size();
        self.check_and_record_alloc(size)?;
        let id = self.next_handle()?;
        self.objects.insert(id, obj);
        Ok(id)
    }

    pub fn get_instance_field(&self, object_id: u32, field_name: &str) -> Result<Value, HeapError> {
        let obj = self.get(object_id)?;
        match obj {
            HeapObject::Instance(inst) => {
                Ok(inst.fields.get(field_name).copied().unwrap_or(Value::Int(0)))
            }
            _ => Err(HeapError::TypeMismatch {
                expected: "InstanceObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    pub fn set_instance_field(&mut self, object_id: u32, field_name: &str, value: Value) -> Result<(), HeapError> {
        let obj = self.get_mut(object_id)?;
        match obj {
            HeapObject::Instance(inst) => {
                inst.fields.insert(field_name.to_owned(), value);
                Ok(())
            }
            _ => Err(HeapError::TypeMismatch {
                expected: "InstanceObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    // --- Array Allocation and Operations ---

    pub fn allocate_array(&mut self, element_descriptor: &str, length: i32) -> Result<u32, HeapError> {
        if length < 0 {
            return Err(HeapError::NegativeArraySize(length));
        }
        let ulen = length as usize;
        let default_val = if element_descriptor.starts_with('L') || element_descriptor.starts_with('[') {
            Value::Null
        } else {
            Value::Int(0)
        };
        let elements = vec![default_val; ulen];
        let obj = HeapObject::Array(ArrayObject {
            element_descriptor: element_descriptor.to_owned(),
            elements,
        });
        let size = obj.approximate_size();
        self.check_and_record_alloc(size)?;
        let id = self.next_handle()?;
        self.objects.insert(id, obj);
        Ok(id)
    }

    pub fn array_length(&self, array_id: u32) -> Result<usize, HeapError> {
        let obj = self.get(array_id)?;
        match obj {
            HeapObject::Array(arr) => Ok(arr.elements.len()),
            _ => Err(HeapError::TypeMismatch {
                expected: "ArrayObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    pub fn get_array_element(&self, array_id: u32, index: i32) -> Result<Value, HeapError> {
        let obj = self.get(array_id)?;
        match obj {
            HeapObject::Array(arr) => {
                if index < 0 || (index as usize) >= arr.elements.len() {
                    return Err(HeapError::ArrayIndexOutOfBounds {
                        index,
                        length: arr.elements.len(),
                    });
                }
                Ok(arr.elements[index as usize])
            }
            _ => Err(HeapError::TypeMismatch {
                expected: "ArrayObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    pub fn set_array_element(&mut self, array_id: u32, index: i32, value: Value) -> Result<(), HeapError> {
        let obj = self.get_mut(array_id)?;
        match obj {
            HeapObject::Array(arr) => {
                if index < 0 || (index as usize) >= arr.elements.len() {
                    return Err(HeapError::ArrayIndexOutOfBounds {
                        index,
                        length: arr.elements.len(),
                    });
                }
                arr.elements[index as usize] = value;
                Ok(())
            }
            _ => Err(HeapError::TypeMismatch {
                expected: "ArrayObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    // --- String Allocation and Operations ---

    pub fn allocate_string(&mut self, content: &str) -> Result<u32, HeapError> {
        let obj = HeapObject::String(StringObject {
            content: content.to_owned(),
        });
        let size = obj.approximate_size();
        self.check_and_record_alloc(size)?;
        let id = self.next_handle()?;
        self.objects.insert(id, obj);
        Ok(id)
    }

    pub fn get_string(&self, string_id: u32) -> Result<&str, HeapError> {
        let obj = self.get(string_id)?;
        match obj {
            HeapObject::String(s) => Ok(&s.content),
            _ => Err(HeapError::TypeMismatch {
                expected: "StringObject".into(),
                actual: obj.class_descriptor().into(),
            }),
        }
    }

    // --- Real Mark-Sweep Garbage Collection ---

    pub fn collect_garbage(&mut self, roots: &[u32]) -> GcReport {
        let before_count = self.objects.len();
        let mut marked = HashSet::new();
        let mut queue = Vec::new();

        for &root in roots {
            if root != 0 && self.objects.contains_key(&root) && marked.insert(root) {
                queue.push(root);
            }
        }

        while let Some(current_id) = queue.pop() {
            if let Some(obj) = self.objects.get(&current_id) {
                match obj {
                    HeapObject::Instance(inst) => {
                        for val in inst.fields.values() {
                            if let Some(child_id) = val.as_object() {
                                if child_id != 0 && self.objects.contains_key(&child_id) && marked.insert(child_id) {
                                    queue.push(child_id);
                                }
                            }
                        }
                    }
                    HeapObject::Array(arr) => {
                        for val in &arr.elements {
                            if let Some(child_id) = val.as_object() {
                                if child_id != 0 && self.objects.contains_key(&child_id) && marked.insert(child_id) {
                                    queue.push(child_id);
                                }
                            }
                        }
                    }
                    HeapObject::String(_) => {}
                }
            }
        }

        let mut reclaimed_bytes = 0usize;
        let mut to_remove = Vec::new();

        for (&id, obj) in &self.objects {
            if !marked.contains(&id) {
                to_remove.push((id, obj.approximate_size()));
            }
        }

        for (id, size) in to_remove {
            self.objects.remove(&id);
            reclaimed_bytes += size;
            self.allocated_bytes = self.allocated_bytes.saturating_sub(size);
        }

        self.gc_runs += 1;
        self.total_reclaimed_bytes += reclaimed_bytes;

        GcReport {
            collections_performed: self.gc_runs,
            objects_before: before_count,
            objects_after: self.objects.len(),
            bytes_reclaimed: reclaimed_bytes,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_allocates_and_reclaims_dead_objects() {
        let mut heap = ManagedHeap::new(1024 * 1024);
        let s1 = heap.allocate_string("Str1").unwrap();
        let s2 = heap.allocate_string("Str2").unwrap();
        let arr = heap.allocate_array("[I", 3).unwrap();
        let inst = heap.allocate_instance("Ltest/Foo;").unwrap();

        heap.set_instance_field(inst, "ref", Value::Object(s1)).unwrap();

        // GC with root = inst
        let report = heap.collect_garbage(&[inst]);
        assert_eq!(report.objects_before, 4);
        assert_eq!(report.objects_after, 2); // inst and s1 survive
        assert!(heap.contains(inst));
        assert!(heap.contains(s1));
        assert!(!heap.contains(s2));
        assert!(!heap.contains(arr));
    }

    #[test]
    fn heap_enforces_array_bounds() {
        let mut heap = ManagedHeap::new(1024 * 1024);
        let arr = heap.allocate_array("[I", 2).unwrap();
        assert_eq!(heap.array_length(arr).unwrap(), 2);
        assert!(matches!(
            heap.get_array_element(arr, 5),
            Err(HeapError::ArrayIndexOutOfBounds { .. })
        ));
    }
}
