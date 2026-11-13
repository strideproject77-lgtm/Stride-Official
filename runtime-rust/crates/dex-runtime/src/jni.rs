//! JNI native compatibility bridge and method dispatch boundary.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use crate::types::Value;
use crate::heap::{GcReport, ManagedHeap};


// Process-relative monotonic clock used for Android uptime APIs. Unlike wall-clock
// time, this source cannot jump when the device clock or time zone is adjusted.
fn monotonic_uptime_millis() -> i64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_millis().min(i64::MAX as u128) as i64
}

pub type NativeHandler = Arc<dyn Fn(&mut JniEnv, &[Value]) -> Result<Option<Value>, String> + Send + Sync>;

/// Opaque JNI reference handles. Guest-visible tokens are deliberately distinct
/// from managed-heap object IDs, so local/global lifetime rules can be enforced
/// without exposing Rust heap handles as JNI jobject values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuestReferenceKind {
    Local,
    Global,
    WeakGlobal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GuestReference {
    object_id: u32,
    kind: GuestReferenceKind,
}

/// Reference-token registry for the future guest JNIEnv ABI bridge.
/// Tokens are opaque integer handles, never host pointers.
#[derive(Debug, Clone)]
pub struct JniReferenceTable {
    entries: HashMap<u64, GuestReference>,
    local_frames: Vec<Vec<u64>>,
    next_handle: u64,
}

impl Default for JniReferenceTable {
    fn default() -> Self {
        // Keep opaque guest tokens outside the 32-bit managed-object ID range.
        // A token must never be mistaken for a raw heap object handle by bridge code.
        Self { entries: HashMap::new(), local_frames: vec![Vec::new()], next_handle: 1u64 << 32 }
    }
}

impl JniReferenceTable {
    pub fn new() -> Self { Self::default() }

    fn allocate(&mut self, object_id: u32, kind: GuestReferenceKind) -> Result<u64, String> {
        if object_id == 0 { return Ok(0); }
        let handle = self.next_handle;
        let next_handle = self.next_handle.checked_add(1)
            .ok_or_else(|| "JNI opaque reference handle space exhausted".to_owned())?;
        self.entries.try_reserve(1)
            .map_err(|_| "JNI opaque reference table allocation failed".to_owned())?;
        if kind == GuestReferenceKind::Local {
            let frame = self.local_frames.last_mut()
                .ok_or_else(|| "JNI local-reference frame stack is empty".to_owned())?;
            frame.try_reserve(1)
                .map_err(|_| "JNI local-reference frame allocation failed".to_owned())?;
            frame.push(handle);
        }
        self.entries.insert(handle, GuestReference { object_id, kind });
        self.next_handle = next_handle;
        Ok(handle)
    }

    pub fn new_local(&mut self, object_id: u32) -> Result<u64, String> {
        self.allocate(object_id, GuestReferenceKind::Local)
    }

    pub fn new_global(&mut self, object_id: u32) -> Result<u64, String> {
        self.allocate(object_id, GuestReferenceKind::Global)
    }

    pub fn new_weak_global(&mut self, object_id: u32) -> Result<u64, String> {
        self.allocate(object_id, GuestReferenceKind::WeakGlobal)
    }

    pub fn resolve(&self, handle: u64) -> Result<Option<u32>, String> {
        if handle == 0 { return Ok(None); }
        self.entries.get(&handle)
            .map(|entry| if entry.object_id == 0 { None } else { Some(entry.object_id) })
            .ok_or_else(|| format!("JNI opaque reference handle {handle} is invalid"))
    }

    pub fn is_same_object(&self, left: u64, right: u64) -> Result<bool, String> {
        Ok(self.resolve(left)? == self.resolve(right)?)
    }

    pub fn delete_local(&mut self, handle: u64) -> Result<(), String> {
        if handle == 0 { return Ok(()); }
        let entry = self.entries.get(&handle)
            .ok_or_else(|| format!("JNI local reference {handle} is invalid"))?;
        if entry.kind != GuestReferenceKind::Local {
            return Err(format!("JNI reference {handle} is not local"));
        }
        self.entries.remove(&handle);
        if let Some(frame) = self.local_frames.iter_mut().rev().find(|frame| frame.contains(&handle)) {
            frame.retain(|candidate| *candidate != handle);
        }
        Ok(())
    }

    pub fn delete_global(&mut self, handle: u64) -> Result<(), String> {
        self.delete_non_local(handle, GuestReferenceKind::Global)
    }

    pub fn delete_weak_global(&mut self, handle: u64) -> Result<(), String> {
        self.delete_non_local(handle, GuestReferenceKind::WeakGlobal)
    }

    fn delete_non_local(&mut self, handle: u64, expected: GuestReferenceKind) -> Result<(), String> {
        if handle == 0 { return Ok(()); }
        let entry = self.entries.get(&handle)
            .ok_or_else(|| format!("JNI reference {handle} is invalid"))?;
        if entry.kind != expected {
            return Err(format!("JNI reference {handle} has the wrong lifetime kind"));
        }
        self.entries.remove(&handle);
        Ok(())
    }

    pub fn push_local_frame(&mut self, capacity: usize) -> Result<(), String> {
        let mut frame = Vec::new();
        frame.try_reserve(capacity)
            .map_err(|_| "JNI guest local-frame allocation failed".to_owned())?;
        self.local_frames.try_reserve(1)
            .map_err(|_| "JNI guest frame stack allocation failed".to_owned())?;
        self.local_frames.push(frame);
        Ok(())
    }

    pub fn pop_local_frame(&mut self, result: u64) -> Result<u64, String> {
        if self.local_frames.len() <= 1 {
            return Err("JNI guest PopLocalFrame cannot pop the root frame".to_owned());
        }
        let result_object = self.resolve(result)?;
        // Reserve the destination frame and entry slot before dropping the old
        // frame, so allocation failure cannot destroy a valid result reference.
        if result_object.is_some() {
            let parent_index = self.local_frames.len() - 2;
            self.local_frames[parent_index].try_reserve(1)
                .map_err(|_| "JNI parent local-frame allocation failed".to_owned())?;
            self.entries.try_reserve(1)
                .map_err(|_| "JNI result reference allocation failed".to_owned())?;
            self.next_handle.checked_add(1)
                .ok_or_else(|| "JNI opaque reference handle space exhausted".to_owned())?;
        }
        let frame = self.local_frames.pop().expect("frame count checked");
        for handle in frame { self.entries.remove(&handle); }
        match result_object {
            Some(object_id) => self.new_local(object_id),
            None => Ok(0),
        }
    }

    /// Clear weak referents reclaimed by GC while keeping their JNI handles valid.
    /// A cleared weak reference resolves as NULL and can still be deleted normally.
    pub fn prune_collected_weak_refs(&mut self, heap: &ManagedHeap) -> usize {
        let mut cleared = 0;
        for entry in self.entries.values_mut() {
            if entry.kind == GuestReferenceKind::WeakGlobal
                && entry.object_id != 0
                && !heap.contains(entry.object_id)
            {
                entry.object_id = 0;
                cleared += 1;
            }
        }
        cleared
    }

    pub fn strong_roots(&self, heap: &ManagedHeap) -> Vec<u32> {
        self.entries.values().filter_map(|entry| {
            (entry.kind != GuestReferenceKind::WeakGlobal && heap.contains(entry.object_id))
                .then_some(entry.object_id)
        }).collect()
    }

    pub fn local_frame_depth(&self) -> usize { self.local_frames.len() }
    pub fn live_reference_count(&self) -> usize { self.entries.len() }
}


/// Stable slot numbers from the JNI 1.x native interface prefix. These are
/// ABI indices, not callable implementations; a slot is usable only after a
/// real guest callback address has been installed by the runtime.
pub mod jni_abi_slots {
    pub const RESERVED0: usize = 0;
    pub const RESERVED1: usize = 1;
    pub const RESERVED2: usize = 2;
    pub const RESERVED3: usize = 3;
    pub const GET_VERSION: usize = 4;
    pub const DEFINE_CLASS: usize = 5;
    pub const FIND_CLASS: usize = 6;
    pub const FROM_REFLECTED_METHOD: usize = 7;
    pub const FROM_REFLECTED_FIELD: usize = 8;
    pub const TO_REFLECTED_METHOD: usize = 9;
    pub const GET_SUPERCLASS: usize = 10;
    pub const IS_ASSIGNABLE_FROM: usize = 11;
    pub const TO_REFLECTED_FIELD: usize = 12;
    pub const THROW: usize = 13;
    pub const THROW_NEW: usize = 14;
    pub const EXCEPTION_OCCURRED: usize = 15;
    pub const EXCEPTION_DESCRIBE: usize = 16;
    pub const EXCEPTION_CLEAR: usize = 17;
    pub const FATAL_ERROR: usize = 18;
    pub const PUSH_LOCAL_FRAME: usize = 19;
    pub const POP_LOCAL_FRAME: usize = 20;
    pub const NEW_GLOBAL_REF: usize = 21;
    pub const DELETE_GLOBAL_REF: usize = 22;
    pub const DELETE_LOCAL_REF: usize = 23;
    pub const IS_SAME_OBJECT: usize = 24;
    pub const NEW_LOCAL_REF: usize = 25;
    pub const ENSURE_LOCAL_CAPACITY: usize = 26;
}

pub mod java_vm_abi_slots {
    pub const DESTROY_JAVA_VM: usize = 3;
    pub const ATTACH_CURRENT_THREAD: usize = 4;
    pub const DETACH_CURRENT_THREAD: usize = 5;
    pub const GET_ENV: usize = 6;
    pub const ATTACH_CURRENT_THREAD_AS_DAEMON: usize = 7;
}

pub const JNI_NATIVE_INTERFACE_MIN_SLOTS: usize = 27;
pub const JAVA_VM_INVOCATION_INTERFACE_MIN_SLOTS: usize = 8;
pub const JNI_ABI_MAX_SLOTS: usize = 4096;

/// Guest-address function table storage for the JNI C ABI. A JNIEnv* points to
/// an environment cell containing the address of this table; it does not point
/// directly at the first function pointer. The table stores guest code
/// addresses only—never Rust closures or host function pointers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JniAbiFunctionTable {
    entries: Vec<u64>,
}

impl JniAbiFunctionTable {
    pub fn new(slot_count: usize) -> Result<Self, String> {
        if slot_count == 0 || slot_count > JNI_ABI_MAX_SLOTS {
            return Err(format!("invalid JNI ABI function-table size: {slot_count}"));
        }
        let mut entries = Vec::new();
        entries.try_reserve_exact(slot_count)
            .map_err(|_| "JNI ABI function-table allocation failed".to_owned())?;
        entries.resize(slot_count, 0);
        Ok(Self { entries })
    }

    pub fn native_interface() -> Result<Self, String> {
        Self::new(JNI_NATIVE_INTERFACE_MIN_SLOTS)
    }

    pub fn java_vm_invocation_interface() -> Result<Self, String> {
        Self::new(JAVA_VM_INVOCATION_INTERFACE_MIN_SLOTS)
    }

    pub fn slot_count(&self) -> usize { self.entries.len() }

    pub fn install(&mut self, slot: usize, guest_function_address: u64) -> Result<(), String> {
        if slot >= self.entries.len() {
            return Err(format!("JNI ABI slot {slot} is outside the allocated table"));
        }
        if guest_function_address == 0 || guest_function_address & 3 != 0 {
            return Err("JNI ABI callback must be a non-null, 4-byte-aligned guest address".into());
        }
        self.entries[slot] = guest_function_address;
        Ok(())
    }

    pub fn address(&self, slot: usize) -> Option<u64> {
        self.entries.get(slot).copied().filter(|address| *address != 0)
    }

    pub fn require_slots(&self, required: &[usize]) -> Result<(), String> {
        for slot in required {
            match self.entries.get(*slot) {
                Some(address) if *address != 0 => {}
                Some(_) => return Err(format!("required JNI ABI slot {slot} has no callback")),
                None => return Err(format!("required JNI ABI slot {slot} is outside the table")),
            }
        }
        Ok(())
    }

    /// Serialize as the little-endian 64-bit pointer array expected by AArch64.
    /// Zero entries remain visibly unsupported; callers must not advertise
    /// them as implemented callbacks.
    pub fn encode_le(&self) -> Result<Vec<u8>, String> {
        let byte_count = self.entries.len().checked_mul(8)
            .ok_or_else(|| "JNI ABI table byte length overflow".to_owned())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(byte_count)
            .map_err(|_| "JNI ABI table serialization allocation failed".to_owned())?;
        for address in &self.entries {
            bytes.extend_from_slice(&address.to_le_bytes());
        }
        Ok(bytes)
    }

    /// A C JNIEnv*/JavaVM* is a pointer to a one-word environment cell whose
    /// contents point to the function table.
    pub fn encode_environment_cell(table_guest_address: u64) -> Result<[u8; 8], String> {
        if table_guest_address == 0 || table_guest_address & 7 != 0 {
            return Err("JNI ABI table address must be non-null and 8-byte aligned".into());
        }
        Ok(table_guest_address.to_le_bytes())
    }
}

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
    /// Strong JNI global references, retained independently of local frames.
    pub global_refs: Vec<u32>,
    /// Opaque guest jobject tokens, separate from internal heap object IDs.
    pub guest_references: JniReferenceTable,
    pub local_frames: Vec<usize>,
    /// Android package identity supplied by the app/package bootstrap layer.
    package_name: Option<String>,
}

impl<'a> JniEnv<'a> {
    pub fn new(heap: &'a mut ManagedHeap) -> Self {
        Self {
            heap,
            local_refs: Vec::new(),
            global_refs: Vec::new(),
            guest_references: JniReferenceTable::new(),
            local_frames: Vec::new(),
            package_name: None,
        }
    }

    /// Configure the package identity exposed by Android Context bridges.
    pub fn set_package_name(&mut self, package_name: impl Into<String>) -> Result<(), String> {
        let package_name = package_name.into();
        if package_name.is_empty()
            || package_name.split('.').any(|part| part.is_empty()
                || !part.chars().enumerate().all(|(index, ch)| {
                    if index == 0 { ch == '_' || ch.is_ascii_alphabetic() }
                    else { ch == '_' || ch.is_ascii_alphanumeric() }
                }))
        {
            return Err(format!("invalid Android package name: {package_name}"));
        }
        self.package_name = Some(package_name);
        Ok(())
    }

    pub fn push_local_frame(&mut self, _capacity: usize) {
        self.local_frames.push(self.local_refs.len());
    }

    pub fn pop_local_frame(&mut self) {
        if let Some(prev_len) = self.local_frames.pop() {
            self.local_refs.truncate(prev_len);
        }
    }

    /// Push a JNI local-reference frame while reserving capacity up front.
    pub fn try_push_local_frame(&mut self, capacity: usize) -> Result<(), String> {
        self.local_refs.try_reserve(capacity)
            .map_err(|_| "JNI local-reference frame allocation failed".to_owned())?;
        self.local_frames.push(self.local_refs.len());
        Ok(())
    }

    /// Add another local reference to an existing managed object.
    pub fn new_local_ref(&mut self, object_id: u32) -> Result<u32, String> {
        // JNI NewLocalRef(NULL) is defined to return NULL.
        if object_id == 0 {
            return Ok(0);
        }
        if !self.heap.contains(object_id) {
            return Err(format!("JNI NewLocalRef received invalid object handle {object_id}"));
        }
        self.local_refs.try_reserve(1)
            .map_err(|_| "JNI local-reference allocation failed".to_owned())?;
        self.local_refs.push(object_id);
        Ok(object_id)
    }

    /// Delete one matching local-reference entry without freeing the object.
    pub fn delete_local_ref(&mut self, object_id: u32) -> Result<(), String> {
        if object_id == 0 { return Ok(()); }
        let Some(index) = self.local_refs.iter().rposition(|&id| id == object_id) else {
            return Err(format!("JNI DeleteLocalRef received unknown local reference {object_id}"));
        };
        self.local_refs.remove(index);
        Ok(())
    }

    /// Return the handles that must be treated as JNI local GC roots.
    pub fn local_reference_roots(&self) -> Vec<u32> {
        self.local_refs.iter().copied().filter(|&id| id != 0 && self.heap.contains(id)).collect()
    }

    /// Create a strong global reference that survives local-frame pops.
    pub fn new_global_ref(&mut self, object_id: u32) -> Result<u32, String> {
        if object_id == 0 {
            return Ok(0);
        }
        if !self.heap.contains(object_id) {
            return Err(format!("JNI NewGlobalRef received invalid object handle {object_id}"));
        }
        self.global_refs.try_reserve(1)
            .map_err(|_| "JNI global-reference allocation failed".to_owned())?;
        self.global_refs.push(object_id);
        Ok(object_id)
    }

    /// Delete one strong global reference without freeing the managed object.
    pub fn delete_global_ref(&mut self, object_id: u32) -> Result<(), String> {
        if object_id == 0 {
            return Ok(());
        }
        let Some(index) = self.global_refs.iter().rposition(|&id| id == object_id) else {
            return Err(format!("JNI DeleteGlobalRef received unknown global reference {object_id}"));
        };
        self.global_refs.remove(index);
        Ok(())
    }

    /// Snapshot strong global roots for the managed heap collector.
    pub fn global_reference_roots(&self) -> Vec<u32> {
        self.global_refs.iter().copied().filter(|&id| id != 0 && self.heap.contains(id)).collect()
    }

    /// Snapshot both local and global strong JNI roots for a GC cycle.
    pub fn strong_reference_roots(&self) -> Vec<u32> {
        let mut roots = self.local_reference_roots();
        roots.extend(self.global_reference_roots());
        // A managed object may be held by both a local and a global reference.
        // The collector needs the object once, not one root entry per JNI handle.
        roots.sort_unstable();
        roots.dedup();
        roots
    }

    /// Create an opaque guest jobject token for an internal managed object.
    pub fn new_guest_local_ref(&mut self, object_id: u32) -> Result<u64, String> {
        if object_id != 0 && !self.heap.contains(object_id) {
            return Err(format!("JNI guest local reference received invalid object {object_id}"));
        }
        self.guest_references.new_local(object_id)
    }

    pub fn new_guest_global_ref(&mut self, object_id: u32) -> Result<u64, String> {
        if object_id != 0 && !self.heap.contains(object_id) {
            return Err(format!("JNI guest global reference received invalid object {object_id}"));
        }
        self.guest_references.new_global(object_id)
    }

    pub fn new_guest_weak_global_ref(&mut self, object_id: u32) -> Result<u64, String> {
        if object_id != 0 && !self.heap.contains(object_id) {
            return Err(format!("JNI guest weak reference received invalid object {object_id}"));
        }
        self.guest_references.new_weak_global(object_id)
    }

    /// Resolve a guest-visible opaque jobject token to an internal heap handle.
    pub fn resolve_guest_ref(&self, guest_handle: u64) -> Result<Option<u32>, String> {
        let object_id = self.guest_references.resolve(guest_handle)?;
        if let Some(id) = object_id {
            if !self.heap.contains(id) {
                // A cleared weak global behaves exactly like NULL. Strong
                // references to reclaimed objects indicate a broken root/lifetime
                // invariant and must remain an explicit error.
                let is_weak = self.guest_references.entries
                    .get(&guest_handle)
                    .is_some_and(|entry| entry.kind == GuestReferenceKind::WeakGlobal);
                if is_weak {
                    return Ok(None);
                }
                return Err(format!("JNI guest reference {guest_handle} points to a collected object"));
            }
        }
        Ok(object_id)
    }

    pub fn new_guest_local_ref_from_handle(&mut self, guest_handle: u64) -> Result<u64, String> {
        let object_id = self.resolve_guest_ref(guest_handle)?.unwrap_or(0);
        self.new_guest_local_ref(object_id)
    }

    pub fn new_guest_global_ref_from_handle(&mut self, guest_handle: u64) -> Result<u64, String> {
        let object_id = self.resolve_guest_ref(guest_handle)?.unwrap_or(0);
        self.new_guest_global_ref(object_id)
    }

    pub fn new_guest_weak_global_ref_from_handle(&mut self, guest_handle: u64) -> Result<u64, String> {
        let object_id = self.resolve_guest_ref(guest_handle)?.unwrap_or(0);
        self.new_guest_weak_global_ref(object_id)
    }

    pub fn delete_guest_local_ref(&mut self, guest_handle: u64) -> Result<(), String> {
        self.guest_references.delete_local(guest_handle)
    }

    pub fn delete_guest_global_ref(&mut self, guest_handle: u64) -> Result<(), String> {
        self.guest_references.delete_global(guest_handle)
    }

    pub fn delete_guest_weak_global_ref(&mut self, guest_handle: u64) -> Result<(), String> {
        self.guest_references.delete_weak_global(guest_handle)
    }

    pub fn guest_refs_are_same(&self, left: u64, right: u64) -> Result<bool, String> {
        // A stale weak reference behaves as NULL once its referent has been collected,
        // but an unknown opaque token remains an error rather than silently becoming NULL.
        let resolve_live = |handle| -> Result<Option<u32>, String> {
            let object = self.guest_references.resolve(handle)?;
            Ok(object.filter(|id| self.heap.contains(*id)))
        };
        Ok(resolve_live(left)? == resolve_live(right)?)
    }

    /// Snapshot roots from both the legacy internal API and opaque guest handles.
    pub fn all_strong_reference_roots(&self) -> Vec<u32> {
        let mut roots = self.strong_reference_roots();
        roots.extend(self.guest_references.strong_roots(self.heap));
        roots.sort_unstable();
        roots.dedup();
        roots
    }

    /// Run a managed-heap collection with every live JNI strong reference
    /// included automatically. Caller-owned VM/register/stack roots are passed
    /// separately; weak JNI references are cleared after the sweep.
    pub fn collect_garbage(&mut self, additional_roots: &[u32]) -> GcReport {
        let mut roots = additional_roots.to_vec();
        roots.extend(self.all_strong_reference_roots());
        roots.sort_unstable();
        roots.dedup();

        let report = self.heap.collect_garbage(&roots);
        self.guest_references.prune_collected_weak_refs(self.heap);
        report
    }

    /// Pop one guest local frame, preserving its result as a fresh parent-frame handle.
    pub fn pop_guest_local_frame(&mut self, result: u64) -> Result<u64, String> {
        self.guest_references.pop_local_frame(result)
    }

    /// Pop the current frame and optionally preserve one result reference in its parent.
    pub fn pop_local_frame_with_result(&mut self, result: Option<u32>) -> Result<Option<u32>, String> {
        // Validate and reserve before changing either stack so failures are atomic.
        let Some(previous_len) = self.local_frames.last().copied() else {
            return Err("JNI PopLocalFrame called without a pushed local frame".into());
        };
        let preserved = match result {
            Some(0) | None => None,
            Some(object_id) => {
                if !self.heap.contains(object_id) {
                    return Err(format!("JNI PopLocalFrame received invalid result handle {object_id}"));
                }
                self.local_refs.try_reserve(1)
                    .map_err(|_| "JNI result local-reference allocation failed".to_owned())?;
                Some(object_id)
            }
        };
        self.local_frames.pop();
        self.local_refs.truncate(previous_len);
        if let Some(object_id) = preserved {
            self.local_refs.push(object_id);
            Ok(Some(object_id))
        } else {
            Ok(None)
        }
    }

    pub fn new_string_utf(&mut self, text: &str) -> Result<u32, String> {
        self.local_refs.try_reserve(1)
            .map_err(|_| "JNI string local-reference allocation failed".to_owned())?;
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
        self.local_refs.try_reserve(1)
            .map_err(|_| "JNI array local-reference allocation failed".to_owned())?;
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
        self.local_refs.try_reserve(1)
            .map_err(|_| "JNI array local-reference allocation failed".to_owned())?;
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
            let Some(receiver) = args.first().and_then(|value| value.as_object()) else {
                return Ok(Some(Value::Int(0)));
            };
            let left = env.get_string_utf(receiver)?.to_owned();
            let right = args.get(1).and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if left == right { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "length", "()I", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.length receiver is null")?;
            let text = env.get_string_utf(receiver)?;
            Ok(Some(Value::Int(text.encode_utf16().count() as i32)))
        });
        self.register("Ljava/lang/String;", "isEmpty", "()Z", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.isEmpty receiver is null")?;
            Ok(Some(Value::Int(i32::from(env.get_string_utf(receiver)?.is_empty()))))
        });
        self.register("Ljava/lang/String;", "startsWith", "(Ljava/lang/String;)Z", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.startsWith receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let prefix = args.get(1).and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if text.starts_with(prefix) { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "contains", "(Ljava/lang/CharSequence;)Z", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.contains receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let needle = args.get(1).and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("");
            Ok(Some(Value::Int(if text.contains(needle) { 1 } else { 0 })))
        });
        self.register("Ljava/lang/String;", "substring", "(I)Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.substring receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let start = args.get(1).map(Value::as_int).unwrap_or(0).max(0) as usize;
            let suffix = text.chars().skip(start).collect::<String>();
            Ok(Some(Value::Object(env.new_string_utf(&suffix)?)))
        });
        self.register("Ljava/lang/String;", "substring", "(II)Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.substring receiver is null")?;
            let text = env.get_string_utf(receiver)?.to_owned();
            let chars = text.chars().collect::<Vec<_>>();
            let start = args.get(1).map(Value::as_int).unwrap_or(0).max(0) as usize;
            let end = args.get(2).map(Value::as_int).unwrap_or(chars.len() as i32).max(0) as usize;
            let start = start.min(chars.len());
            let end = end.max(start).min(chars.len());
            let result = chars[start..end].iter().collect::<String>();
            Ok(Some(Value::Object(env.new_string_utf(&result)?)))
        });
        self.register("Ljava/lang/String;", "concat", "(Ljava/lang/String;)Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object()).ok_or("String.concat receiver is null")?;
            let left = env.get_string_utf(receiver)?.to_owned();
            let right = args.get(1).and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("").to_owned();
            Ok(Some(Value::Object(env.new_string_utf(&format!("{}{}", left, right))?)))
        });
        self.register("Ljava/lang/String;", "valueOf", "(D)Ljava/lang/String;", |env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Object(env.new_string_utf(&value.to_string())?)))
        });
        self.register("Ljava/lang/String;", "valueOf", "(J)Ljava/lang/String;", |env, args| {
            let value = args.first().map(Value::as_long).unwrap_or(0);
            Ok(Some(Value::Object(env.new_string_utf(&value.to_string())?)))
        });
        self.register("Ljava/lang/Double;", "parseDouble", "(Ljava/lang/String;)D", |env, args| {
            let value = args.first().and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok())
                .and_then(|text| text.parse::<f64>().ok()).unwrap_or(0.0);
            Ok(Some(Value::Double(value)))
        });
        self.register("Ljava/lang/Double;", "isNaN", "(D)Z", |_env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Int(i32::from(value.is_nan()))))
        });
        self.register("Ljava/lang/Double;", "isInfinite", "(D)Z", |_env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Int(i32::from(value.is_infinite()))))
        });

        // Android Context package identity. The bootstrap layer must set the
        // package name for each app; never silently return a placeholder package.
        self.register(
            "Landroid/content/Context;",
            "getPackageName",
            "()Ljava/lang/String;",
            |env, _args| {
                let package_name = env.package_name.clone()
                    .ok_or("Context.getPackageName called before app package identity was configured")?;
                Ok(Some(Value::Object(env.new_string_utf(&package_name)?)))
            },
        );
        self.register(
            "Landroid/content/Context;",
            "getOpPackageName",
            "()Ljava/lang/String;",
            |env, _args| {
                let package_name = env.package_name.clone()
                    .ok_or("Context.getOpPackageName called before app package identity was configured")?;
                Ok(Some(Value::Object(env.new_string_utf(&package_name)?)))
            },
        );

        // android.os.SystemClock.uptimeMillis()J. Android uptime is monotonic;
        // wall-clock epoch time belongs to System.currentTimeMillis instead.
        self.register(
            "Landroid/os/SystemClock;",
            "uptimeMillis",
            "()J",
            |_env, _args| Ok(Some(Value::Long(monotonic_uptime_millis()))),
        );

        // android.util.Log logging bridges
        let log_fn = |_env: &mut JniEnv, _args: &[Value]| Ok(Some(Value::Int(1)));
        self.register("Landroid/util/Log;", "i", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "d", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "e", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "w", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);
        self.register("Landroid/util/Log;", "v", "(Ljava/lang/String;Ljava/lang/String;)I", log_fn);

        // java.lang.StringBuilder bridges used by ordinary DEX-generated input handlers.
        // Keep builder contents on the guest object so constructor/append/toString calls
        // participate in the same managed heap as the app's other objects.
        self.register("Ljava/lang/StringBuilder;", "<init>", "()V", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object())
                .ok_or("StringBuilder constructor receiver is null")?;
            let empty = env.new_string_utf("")?;
            env.set_object_field(receiver, "mStrideBuilderValue", Value::Object(empty))?;
            Ok(None)
        });
        self.register("Ljava/lang/StringBuilder;", "<init>", "(Ljava/lang/String;)V", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object())
                .ok_or("StringBuilder constructor receiver is null")?;
            let initial = args.get(1).and_then(|value| value.as_object())
                .map(|id| env.get_string_utf(id).map(str::to_owned))
                .transpose()?.unwrap_or_default();
            let value = env.new_string_utf(&initial)?;
            env.set_object_field(receiver, "mStrideBuilderValue", Value::Object(value))?;
            Ok(None)
        });
        self.register("Ljava/lang/StringBuilder;", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object())
                .ok_or("StringBuilder.append receiver is null")?;
            let current = env.get_object_field(receiver, "mStrideBuilderValue")
                .ok().and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("").to_owned();
            let suffix = args.get(1).and_then(|value| value.as_object())
                .map(|id| env.get_string_utf(id).map(str::to_owned))
                .transpose()?.unwrap_or_else(|| "null".to_owned());
            let updated = env.new_string_utf(&format!("{current}{suffix}"))?;
            env.set_object_field(receiver, "mStrideBuilderValue", Value::Object(updated))?;
            Ok(Some(Value::Object(receiver)))
        });
        self.register("Ljava/lang/StringBuilder;", "toString", "()Ljava/lang/String;", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object())
                .ok_or("StringBuilder.toString receiver is null")?;
            let value = env.get_object_field(receiver, "mStrideBuilderValue")
                .ok().and_then(|value| value.as_object())
                .and_then(|id| env.get_string_utf(id).ok()).unwrap_or("").to_owned();
            Ok(Some(Value::Object(env.new_string_utf(&value)?)))
        });

        // Boxing and formatting are required by the calculator's equals/result path.
        self.register("Ljava/lang/Double;", "valueOf", "(D)Ljava/lang/Double;", |env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            let boxed = env.heap.allocate_instance("Ljava/lang/Double;")
                .map_err(|error| format!("Double.valueOf allocation failed: {:?}", error))?;
            env.set_object_field(boxed, "value", Value::Double(value))?;
            Ok(Some(Value::Object(boxed)))
        });
        self.register("Ljava/lang/String;", "format", "(Ljava/util/Locale;Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;", |env, args| {
            let format_id = args.get(1).and_then(|value| value.as_object())
                .ok_or("String.format format argument is null")?;
            let format_text = env.get_string_utf(format_id)?.to_owned();
            let array_id = args.get(2).and_then(|value| value.as_object())
                .ok_or("String.format argument array is null")?;
            let count = env.get_array_length(array_id)?;
            let mut values = Vec::with_capacity(count);
            for index in 0..count {
                let item = env.heap.get_array_element(array_id, index as i32)
                    .map_err(|error| format!("String.format argument read failed: {:?}", error))?;
                let object_id = item.as_object().ok_or("String.format received a null boxed argument")?;
                let class_name = env.heap.get(object_id)
                    .map_err(|error| format!("String.format boxed argument lookup failed: {:?}", error))?
                    .class_descriptor().to_owned();
                let number = if class_name == "Ljava/lang/Double;" {
                    env.get_object_field(object_id, "value")?.as_double()
                } else {
                    return Err(format!("String.format does not support boxed argument type {class_name}"));
                };
                values.push(number);
            }
            let mut rendered = format_text.clone();
            for number in values {
                let Some(start) = rendered.find("%.") else { break };
                let tail = &rendered[start + 2..];
                let digits = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect::<String>();
                let precision = digits.parse::<usize>().unwrap_or(6).clamp(1, 17);
                let conversion_at = start + 2 + digits.len();
                let Some(conversion) = rendered[conversion_at..].chars().next() else { break };
                let formatted = match conversion {
                    'g' | 'G' => {
                        if number == 0.0 { format!("{:.*}", precision.saturating_sub(1), number) }
                        else if number.is_finite() {
                            let exponent = number.abs().log10().floor() as i32;
                            if exponent < -4 || exponent >= precision as i32 {
                                let raw = format!("{:.*e}", precision - 1, number);
                                if let Some((mantissa, exp)) = raw.split_once('e') {
                                    let exp_value = exp.parse::<i32>().unwrap_or(0);
                                    format!("{mantissa}e{}{magnitude:02}", if exp_value >= 0 { "+" } else { "-" }, magnitude = exp_value.abs())
                                } else { raw }
                            } else {
                                let decimals = (precision as i32 - 1 - exponent).max(0) as usize;
                                format!("{:.*}", decimals, number)
                            }
                        } else { number.to_string() }
                    }
                    'f' | 'F' => format!("{:.*}", precision, number),
                    _ => return Err(format!("String.format conversion %{conversion} is unsupported")),
                };
                let end = conversion_at + conversion.len_utf8();
                rendered.replace_range(start..end, &formatted);
            }
            Ok(Some(Value::Object(env.new_string_utf(&rendered)?)))
        });

        // android.view.MotionEvent accessors. The host queues pointer coordinates
        // on the managed MotionEvent object; guest DEX must read those exact values
        // through the Android API rather than receiving default zero values.
        self.register("Landroid/view/MotionEvent;", "getAction", "()I", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getAction receiver is null")?;
            let action = env.get_object_field(receiver, "action")
                .map(|value| value.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(action)))
        });
        self.register("Landroid/view/MotionEvent;", "getActionMasked", "()I", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getActionMasked receiver is null")?;
            let action = env.get_object_field(receiver, "action")
                .map(|value| value.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(action & 0xff)))
        });
        self.register("Landroid/view/MotionEvent;", "getX", "()F", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getX receiver is null")?;
            let x = env.get_object_field(receiver, "x")
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(x)))
        });
        self.register("Landroid/view/MotionEvent;", "getY", "()F", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getY receiver is null")?;
            let y = env.get_object_field(receiver, "y")
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(y)))
        });
        self.register("Landroid/view/MotionEvent;", "getX", "(I)F", |env, args| {
            if args.get(1).map(Value::as_int).unwrap_or(0) != 0 {
                return Err("MotionEvent.getX only has one pointer in the current input bridge".into());
            }
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getX receiver is null")?;
            let x = env.get_object_field(receiver, "x")
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(x)))
        });
        self.register("Landroid/view/MotionEvent;", "getY", "(I)F", |env, args| {
            if args.get(1).map(Value::as_int).unwrap_or(0) != 0 {
                return Err("MotionEvent.getY only has one pointer in the current input bridge".into());
            }
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getY receiver is null")?;
            let y = env.get_object_field(receiver, "y")
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(y)))
        });
        self.register("Landroid/view/MotionEvent;", "getRawX", "()F", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getRawX receiver is null")?;
            let x = env.get_object_field(receiver, "rawX")
                .or_else(|_| env.get_object_field(receiver, "x"))
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(x)))
        });
        self.register("Landroid/view/MotionEvent;", "getRawY", "()F", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getRawY receiver is null")?;
            let y = env.get_object_field(receiver, "rawY")
                .or_else(|_| env.get_object_field(receiver, "y"))
                .map(|value| value.as_float()).unwrap_or(0.0);
            Ok(Some(Value::Float(y)))
        });
        self.register("Landroid/view/MotionEvent;", "getPressure", "()F", |env, args| {
            let receiver = args.first().and_then(Value::as_object)
                .ok_or("MotionEvent.getPressure receiver is null")?;
            let pressure = env.get_object_field(receiver, "pressure")
                .map(|value| value.as_float()).unwrap_or(1.0);
            Ok(Some(Value::Float(pressure)))
        });
        self.register("Landroid/view/MotionEvent;", "getPointerCount", "()I", |_env, _args| {
            Ok(Some(Value::Int(1)))
        });
        self.register("Landroid/view/MotionEvent;", "getPointerId", "(I)I", |_env, args| {
            let index = args.get(1).map(Value::as_int).unwrap_or(0);
            Ok(Some(Value::Int(if index == 0 { 0 } else { -1 })))
        });

        // java.lang.Math standard operations
        self.register("Ljava/lang/Math;", "abs", "(I)I", |_env, args| {
            let v = args.first().map(|v| v.as_int()).unwrap_or(0);
            Ok(Some(Value::Int(v.abs())))
        });
        self.register("Ljava/lang/Math;", "abs", "(J)J", |_env, args| {
            let v = args.first().map(|v| v.as_long()).unwrap_or(0);
            Ok(Some(Value::Long(v.abs())))
        });
        self.register("Ljava/lang/Math;", "abs", "(D)D", |_env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Double(value.abs())))
        });
        self.register("Ljava/lang/Math;", "rint", "(D)D", |_env, args| {
            let value = args.first().map(Value::as_double).unwrap_or(0.0);
            Ok(Some(Value::Double(value.round_ties_even())))
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
    fn context_package_identity_returns_configured_package_and_rejects_missing_identity() {
        let registry = JniNativeRegistry::new();
        let get_package_name = registry.find(
            "Landroid/content/Context;",
            "getPackageName",
            "()Ljava/lang/String;",
        ).unwrap().clone();
        let get_op_package_name = registry.find(
            "Landroid/content/Context;",
            "getOpPackageName",
            "()Ljava/lang/String;",
        ).unwrap().clone();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);

        assert!(get_package_name(&mut env, &[]).unwrap_err().contains("before app package identity"));

        env.set_package_name("com.stride.fixture").unwrap();
        let package = match get_package_name(&mut env, &[]).unwrap() {
            Some(Value::Object(id)) => env.get_string_utf(id).unwrap().to_owned(),
            other => panic!("unexpected package name result: {:?}", other),
        };
        let op_package = match get_op_package_name(&mut env, &[]).unwrap() {
            Some(Value::Object(id)) => env.get_string_utf(id).unwrap().to_owned(),
            other => panic!("unexpected op package name result: {:?}", other),
        };
        assert_eq!(package, "com.stride.fixture");
        assert_eq!(op_package, package);
        assert!(env.set_package_name("com..invalid").is_err());
    }

    #[test]
    fn system_clock_uptime_is_monotonic_and_not_wall_clock_epoch_time() {
        let registry = JniNativeRegistry::new();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let uptime = registry.find("Landroid/os/SystemClock;", "uptimeMillis", "()J").unwrap().clone();
        let wall_clock = registry.find("Ljava/lang/System;", "currentTimeMillis", "()J").unwrap().clone();

        let first = match uptime(&mut env, &[]).unwrap() {
            Some(Value::Long(value)) => value,
            other => panic!("unexpected uptime result: {:?}", other),
        };
        let wall = match wall_clock(&mut env, &[]).unwrap() {
            Some(Value::Long(value)) => value,
            other => panic!("unexpected wall-clock result: {:?}", other),
        };
        let second = match uptime(&mut env, &[]).unwrap() {
            Some(Value::Long(value)) => value,
            other => panic!("unexpected uptime result: {:?}", other),
        };

        assert!(first >= 0);
        assert!(second >= first);
        assert!(second < wall);
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
    fn motion_event_accessors_return_host_queued_action_and_coordinates() {
        let registry = JniNativeRegistry::new();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let event = env.heap.allocate_instance("Landroid/view/MotionEvent;").unwrap();
        env.set_object_field(event, "action", Value::Int(0x105)).unwrap();
        env.set_object_field(event, "x", Value::Float(435.75)).unwrap();
        env.set_object_field(event, "y", Value::Float(1287.5)).unwrap();
        env.set_object_field(event, "pressure", Value::Float(0.75)).unwrap();

        let action = registry.find("Landroid/view/MotionEvent;", "getAction", "()I").unwrap().clone();
        let masked = registry.find("Landroid/view/MotionEvent;", "getActionMasked", "()I").unwrap().clone();
        let x = registry.find("Landroid/view/MotionEvent;", "getX", "()F").unwrap().clone();
        let y = registry.find("Landroid/view/MotionEvent;", "getY", "()F").unwrap().clone();
        let indexed_y = registry.find("Landroid/view/MotionEvent;", "getY", "(I)F").unwrap().clone();
        let pressure = registry.find("Landroid/view/MotionEvent;", "getPressure", "()F").unwrap().clone();

        assert_eq!(action(&mut env, &[Value::Object(event)]).unwrap(), Some(Value::Int(0x105)));
        assert_eq!(masked(&mut env, &[Value::Object(event)]).unwrap(), Some(Value::Int(5)));
        assert_eq!(x(&mut env, &[Value::Object(event)]).unwrap(), Some(Value::Float(435.75)));
        assert_eq!(y(&mut env, &[Value::Object(event)]).unwrap(), Some(Value::Float(1287.5)));
        assert_eq!(indexed_y(&mut env, &[Value::Object(event), Value::Int(0)]).unwrap(), Some(Value::Float(1287.5)));
        assert_eq!(pressure(&mut env, &[Value::Object(event)]).unwrap(), Some(Value::Float(0.75)));
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
    fn local_reference_delete_and_gc_roots_are_scoped_correctly() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let first = env.new_string_utf("first").unwrap();
        let second = env.new_string_utf("second").unwrap();
        assert_eq!(env.local_reference_roots(), vec![first, second]);
        env.delete_local_ref(first).unwrap();
        assert_eq!(env.local_reference_roots(), vec![second]);
        assert!(env.delete_local_ref(first).is_err());
        let roots = env.local_reference_roots();
        let report = env.heap.collect_garbage(&roots);
        assert_eq!(report.objects_after, 1);
        assert!(env.heap.contains(second));
        assert!(!env.heap.contains(first));
    }

    #[test]
    fn pop_local_frame_preserves_only_the_requested_result_reference() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let outer = env.new_string_utf("outer").unwrap();
        env.try_push_local_frame(4).unwrap();
        let temporary = env.new_string_utf("temporary").unwrap();
        let result = env.new_string_utf("result").unwrap();
        assert_eq!(env.pop_local_frame_with_result(Some(result)).unwrap(), Some(result));
        assert_eq!(env.local_refs, vec![outer, result]);
        assert!(env.heap.contains(temporary));
        assert_eq!(env.get_string_utf(result).unwrap(), "result");
    }

    #[test]
    fn local_frame_operations_reject_invalid_state_and_handles() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        assert!(env.pop_local_frame_with_result(None).is_err());
        assert!(env.new_local_ref(999).is_err());
        env.try_push_local_frame(1).unwrap();
        assert!(env.pop_local_frame_with_result(Some(999)).is_err());
        // A failed pop must leave both frame and references unchanged.
        assert_eq!(env.local_frames.len(), 1);
        assert!(env.pop_local_frame_with_result(None).is_ok());
        assert!(env.local_frames.is_empty());
    }

    #[test]
    fn null_local_reference_matches_jni_null_semantics() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        assert_eq!(env.new_local_ref(0).unwrap(), 0);
        assert!(env.local_refs.is_empty());
        assert!(env.delete_local_ref(0).is_ok());
    }

    #[test]
    fn global_references_survive_local_frame_pop_and_are_gc_roots() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let global = env.new_string_utf("global").unwrap();
        env.new_global_ref(global).unwrap();
        env.try_push_local_frame(2).unwrap();
        let temporary = env.new_string_utf("temporary").unwrap();
        env.pop_local_frame_with_result(None).unwrap();
        assert_eq!(env.local_refs, vec![global]);
        assert_eq!(env.global_reference_roots(), vec![global]);
        assert_eq!(env.strong_reference_roots(), vec![global]);
        env.delete_local_ref(global).unwrap();
        assert_eq!(env.strong_reference_roots(), vec![global]);
        env.delete_global_ref(global).unwrap();
        assert!(env.strong_reference_roots().is_empty());
        assert!(env.heap.contains(temporary));
    }

    #[test]
    fn guest_reference_tokens_enforce_local_global_and_weak_lifetimes() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let object = env.new_string_utf("guest-object").unwrap();
        let local = env.new_guest_local_ref(object).unwrap();
        let global = env.new_guest_global_ref(object).unwrap();
        let weak = env.new_guest_weak_global_ref(object).unwrap();
        assert_ne!(local, object as u64);
        assert_eq!(env.resolve_guest_ref(local).unwrap(), Some(object));
        assert!(env.guest_references.is_same_object(local, global).unwrap());
        env.guest_references.delete_local(local).unwrap();
        assert!(env.resolve_guest_ref(local).is_err());
        assert_eq!(env.guest_references.strong_roots(env.heap), vec![object]);
        env.guest_references.delete_global(global).unwrap();
        assert!(env.guest_references.strong_roots(env.heap).is_empty());
        assert_eq!(env.guest_references.resolve(weak).unwrap(), Some(object));
    }

    #[test]
    fn guest_reference_clone_delete_and_same_object_follow_jni_rules() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let object = env.new_string_utf("same").unwrap();
        let first = env.new_guest_local_ref(object).unwrap();
        let second = env.new_guest_local_ref_from_handle(first).unwrap();
        assert_ne!(first, second);
        assert!(env.guest_refs_are_same(first, second).unwrap());
        let global = env.new_guest_global_ref_from_handle(first).unwrap();
        env.delete_guest_local_ref(first).unwrap();
        assert_eq!(env.resolve_guest_ref(second).unwrap(), Some(object));
        assert_eq!(env.resolve_guest_ref(global).unwrap(), Some(object));
        env.delete_guest_global_ref(global).unwrap();
        env.delete_guest_local_ref(second).unwrap();
        assert!(env.guest_refs_are_same(0, 0).unwrap());
        assert!(env.new_guest_local_ref_from_handle(0).unwrap() == 0);
    }

    #[test]
    fn guest_local_frame_pop_reissues_result_in_parent_frame() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let outer_object = env.new_string_utf("outer").unwrap();
        let inner_object = env.new_string_utf("inner").unwrap();
        let outer = env.new_guest_local_ref(outer_object).unwrap();
        env.guest_references.push_local_frame(3).unwrap();
        let inner = env.new_guest_local_ref(inner_object).unwrap();
        let preserved = env.pop_guest_local_frame(inner).unwrap();
        assert_eq!(env.resolve_guest_ref(outer).unwrap(), Some(outer_object));
        assert_eq!(env.resolve_guest_ref(inner), Err(format!("JNI opaque reference handle {inner} is invalid")));
        assert_eq!(env.resolve_guest_ref(preserved).unwrap(), Some(inner_object));
        assert_eq!(env.guest_references.local_frame_depth(), 1);
    }

    #[test]
    fn guest_weak_references_do_not_become_strong_gc_roots() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let object = env.new_string_utf("weak").unwrap();
        let weak = env.new_guest_weak_global_ref(object).unwrap();
        assert!(env.guest_references.strong_roots(env.heap).is_empty());
        let report = env.heap.collect_garbage(&[]);
        assert_eq!(report.objects_after, 0);
        assert_eq!(env.guest_references.prune_collected_weak_refs(env.heap), 1);
        assert_eq!(env.guest_references.resolve(weak).unwrap(), None);
        env.guest_references.delete_weak_global(weak).unwrap();
    }

    #[test]
    fn stale_weak_reference_promotes_to_null_but_stale_strong_reference_errors() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let weak_object = env.heap.allocate_string("weak-only").unwrap();
        let strong_object = env.heap.allocate_string("strong-unrooted").unwrap();
        let weak = env.new_guest_weak_global_ref(weak_object).unwrap();
        let strong = env.new_guest_local_ref(strong_object).unwrap();

        // Exercise a collector call that bypasses JniEnv's post-GC weak pruning.
        env.heap.collect_garbage(&[]);

        assert_eq!(env.resolve_guest_ref(weak).unwrap(), None);
        assert_eq!(env.new_guest_local_ref_from_handle(weak).unwrap(), 0);
        assert!(env.guest_refs_are_same(weak, 0).unwrap());
        assert!(env.resolve_guest_ref(strong).is_err());
        env.delete_guest_weak_global_ref(weak).unwrap();
    }

    #[test]
    fn jni_aware_collection_keeps_all_strong_roots_and_clears_weak_refs() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);

        let local_root = env.new_string_utf("local-root").unwrap();
        let global_root = env.heap.allocate_string("global-root").unwrap();
        env.new_global_ref(global_root).unwrap();
        let guest_local_root = env.heap.allocate_string("guest-local-root").unwrap();
        env.new_guest_local_ref(guest_local_root).unwrap();
        let guest_global_root = env.heap.allocate_string("guest-global-root").unwrap();
        env.new_guest_global_ref(guest_global_root).unwrap();
        let weak_only = env.heap.allocate_string("weak-only").unwrap();
        let weak = env.new_guest_weak_global_ref(weak_only).unwrap();
        let caller_root = env.heap.allocate_string("caller-root").unwrap();

        let report = env.collect_garbage(&[caller_root]);

        assert_eq!(report.objects_before, 6);
        assert_eq!(report.objects_after, 5);
        assert!(env.heap.contains(local_root));
        assert!(env.heap.contains(global_root));
        assert!(env.heap.contains(guest_local_root));
        assert!(env.heap.contains(guest_global_root));
        assert!(env.heap.contains(caller_root));
        assert!(!env.heap.contains(weak_only));
        assert_eq!(env.resolve_guest_ref(weak).unwrap(), None);
        assert_eq!(env.guest_references.prune_collected_weak_refs(env.heap), 0);
        env.delete_guest_weak_global_ref(weak).unwrap();
        assert!(env.guest_references.resolve(weak).is_err());
    }

    #[test]
    fn strong_jni_root_snapshot_is_sorted_and_deduplicated() {
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);
        let object = env.new_string_utf("shared-root").unwrap();
        env.new_global_ref(object).unwrap();
        env.new_guest_global_ref(object).unwrap();

        assert_eq!(env.all_strong_reference_roots(), vec![object]);
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

        let substring = registry.find(
            "Ljava/lang/String;", "substring", "(II)Ljava/lang/String;",
        ).unwrap();
        let middle = substring(&mut env, &[Value::Object(combined), Value::Int(1), Value::Int(2)])
            .unwrap().and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(middle).unwrap(), "2");

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

        let value_of_long = registry.find(
            "Ljava/lang/String;", "valueOf", "(J)Ljava/lang/String;",
        ).unwrap();
        let integer_text = value_of_long(&mut env, &[Value::Long(-42)]).unwrap()
            .and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(integer_text).unwrap(), "-42");

        let abs_double = registry.find("Ljava/lang/Math;", "abs", "(D)D").unwrap();
        assert_eq!(abs_double(&mut env, &[Value::Double(-4.5)]).unwrap(), Some(Value::Double(4.5)));
        let rint = registry.find("Ljava/lang/Math;", "rint", "(D)D").unwrap();
        assert_eq!(rint(&mut env, &[Value::Double(2.5)]).unwrap(), Some(Value::Double(2.0)));
        let is_nan = registry.find("Ljava/lang/Double;", "isNaN", "(D)Z").unwrap();
        assert_eq!(is_nan(&mut env, &[Value::Double(f64::NAN)]).unwrap(), Some(Value::Int(1)));
        let is_infinite = registry.find("Ljava/lang/Double;", "isInfinite", "(D)Z").unwrap();
        assert_eq!(is_infinite(&mut env, &[Value::Double(f64::INFINITY)]).unwrap(), Some(Value::Int(1)));
        let is_empty = registry.find("Ljava/lang/String;", "isEmpty", "()Z").unwrap();
        let empty = env.new_string_utf("").unwrap();
        assert_eq!(is_empty(&mut env, &[Value::Object(empty)]).unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn calculator_string_builder_and_result_formatting_bridges_work_end_to_end() {
        let registry = JniNativeRegistry::new();
        let mut heap = ManagedHeap::default();
        let mut env = JniEnv::new(&mut heap);

        let builder = env.heap.allocate_instance("Ljava/lang/StringBuilder;").unwrap();
        let initial = env.new_string_utf("12").unwrap();
        let constructor = registry.find("Ljava/lang/StringBuilder;", "<init>", "(Ljava/lang/String;)V").unwrap().clone();
        constructor(&mut env, &[Value::Object(builder), Value::Object(initial)]).unwrap();
        let suffix = env.new_string_utf("3").unwrap();
        let append = registry.find("Ljava/lang/StringBuilder;", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;").unwrap().clone();
        assert_eq!(append(&mut env, &[Value::Object(builder), Value::Object(suffix)]).unwrap(), Some(Value::Object(builder)));
        let to_string = registry.find("Ljava/lang/StringBuilder;", "toString", "()Ljava/lang/String;").unwrap().clone();
        let result = to_string(&mut env, &[Value::Object(builder)]).unwrap().and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(result).unwrap(), "123");

        let boxed = registry.find("Ljava/lang/Double;", "valueOf", "(D)Ljava/lang/Double;").unwrap().clone();
        let boxed_value = boxed(&mut env, &[Value::Double(12.5)]).unwrap().and_then(|value| value.as_object()).unwrap();
        let args = env.heap.allocate_array("[Ljava/lang/Object;", 1).unwrap();
        env.heap.set_array_element(args, 0, Value::Object(boxed_value)).unwrap();
        let format = env.new_string_utf("%.8g").unwrap();
        let format_call = registry.find("Ljava/lang/String;", "format", "(Ljava/util/Locale;Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;").unwrap().clone();
        let rendered = format_call(&mut env, &[Value::Null, Value::Object(format), Value::Object(args)])
            .unwrap().and_then(|value| value.as_object()).unwrap();
        assert_eq!(env.get_string_utf(rendered).unwrap(), "12.500000");
    }

}
