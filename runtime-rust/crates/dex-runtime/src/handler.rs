//! Android Handler, Looper, and MessageQueue emulation.
//!
//! Provides the Android message-passing and scheduling infrastructure used by
//! applications and games for deferred execution, animation callbacks, and
//! inter-thread messaging.

use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use crate::jni::JniNativeRegistry;
use crate::types::Value;

/// A single message in the Android MessageQueue.
#[derive(Debug, Clone)]
pub struct Message {
    /// Target handler ID (0 = unowned).
    pub target_handler: u32,
    /// What code for the message.
    pub what: i32,
    /// Arbitrary integer args.
    pub arg1: i32,
    pub arg2: i32,
    /// Scheduled delivery time.
    pub delivery_at: Instant,
    /// Optional callback identifier for removeCallbacks.
    pub callback_id: Option<u32>,
}

/// Wrapper for Message implementing Ord for a min-heap (earliest delivery first).
#[derive(Debug, Clone)]
struct ScheduledMessage(Message);

impl PartialEq for ScheduledMessage {
    fn eq(&self, other: &Self) -> bool {
        self.0.delivery_at == other.0.delivery_at
    }
}
impl Eq for ScheduledMessage {}

impl PartialOrd for ScheduledMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse: BinaryHeap is max-heap, we want min by delivery_at
        other.0.delivery_at.cmp(&self.0.delivery_at)
    }
}

/// Android MessageQueue — ordered queue of pending messages.
#[derive(Debug, Default)]
pub struct MessageQueue {
    heap: BinaryHeap<ScheduledMessage>,
    next_callback_id: u32,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self { heap: BinaryHeap::new(), next_callback_id: 1 }
    }

    /// Enqueues a message for immediate delivery.
    pub fn enqueue(&mut self, target_handler: u32, what: i32) {
        self.heap.push(ScheduledMessage(Message {
            target_handler,
            what,
            arg1: 0,
            arg2: 0,
            delivery_at: Instant::now(),
            callback_id: None,
        }));
    }

    /// Enqueues a message with a delay; returns its callback_id.
    pub fn enqueue_delayed(&mut self, target_handler: u32, what: i32, delay_ms: u64) -> u32 {
        let cid = self.next_callback_id;
        self.next_callback_id += 1;
        self.heap.push(ScheduledMessage(Message {
            target_handler,
            what,
            arg1: 0,
            arg2: 0,
            delivery_at: Instant::now() + Duration::from_millis(delay_ms),
            callback_id: Some(cid),
        }));
        cid
    }

    /// Removes all callbacks matching a callback_id.
    pub fn remove_callbacks(&mut self, callback_id: u32) {
        let remaining: Vec<_> = self.heap.drain()
            .filter(|m| m.0.callback_id != Some(callback_id))
            .collect();
        for m in remaining { self.heap.push(m); }
    }

    /// Drains all messages that are due for delivery right now.
    pub fn drain_due(&mut self) -> Vec<Message> {
        let now = Instant::now();
        let mut due = Vec::new();
        while let Some(top) = self.heap.peek() {
            if top.0.delivery_at <= now {
                due.push(self.heap.pop().unwrap().0);
            } else {
                break;
            }
        }
        due
    }

    pub fn pending_count(&self) -> usize {
        self.heap.len()
    }
}

/// A registered Handler bound to a specific Looper.
#[derive(Debug, Clone)]
pub struct Handler {
    pub handler_id: u32,
    pub looper_id: u32,
}

impl Handler {
    pub fn new(handler_id: u32, looper_id: u32) -> Self {
        Self { handler_id, looper_id }
    }
}

/// Android Looper — event loop for a thread.
#[derive(Debug)]
pub struct Looper {
    pub looper_id: u32,
    pub is_main: bool,
    pub queue: MessageQueue,
    pub is_running: bool,
}

impl Looper {
    pub fn new(looper_id: u32, is_main: bool) -> Self {
        Self { looper_id, is_main, queue: MessageQueue::new(), is_running: false }
    }

    /// Ticks the looper, draining due messages and returning them for dispatch.
    pub fn tick(&mut self) -> Vec<Message> {
        self.queue.drain_due()
    }

    pub fn quit(&mut self) {
        self.is_running = false;
    }
}

/// Runtime coordinator for all Loopers and Handlers in the guest process.
#[derive(Debug, Default)]
pub struct LooperCoordinator {
    pub loopers: Vec<Looper>,
    pub handlers: Vec<Handler>,
    pub next_looper_id: u32,
    pub next_handler_id: u32,
    pub main_looper_id: Option<u32>,
}

impl LooperCoordinator {
    pub fn new() -> Self {
        let mut coord = Self {
            loopers: Vec::new(),
            handlers: Vec::new(),
            next_looper_id: 1,
            next_handler_id: 1,
            main_looper_id: None,
        };
        let main_id = coord.create_looper(true);
        coord.main_looper_id = Some(main_id);
        coord
    }

    pub fn create_looper(&mut self, is_main: bool) -> u32 {
        let id = self.next_looper_id;
        self.next_looper_id += 1;
        self.loopers.push(Looper::new(id, is_main));
        id
    }

    pub fn create_handler(&mut self, looper_id: u32) -> u32 {
        let id = self.next_handler_id;
        self.next_handler_id += 1;
        self.handlers.push(Handler::new(id, looper_id));
        id
    }

    pub fn get_looper_mut(&mut self, looper_id: u32) -> Option<&mut Looper> {
        self.loopers.iter_mut().find(|l| l.looper_id == looper_id)
    }

    pub fn main_looper_mut(&mut self) -> Option<&mut Looper> {
        let id = self.main_looper_id?;
        self.loopers.iter_mut().find(|l| l.looper_id == id)
    }

    /// Posts a message to the main looper's queue.
    pub fn post_to_main(&mut self, handler_id: u32, what: i32) {
        if let Some(looper) = self.main_looper_mut() {
            looper.queue.enqueue(handler_id, what);
        }
    }

    /// Posts a delayed message to the main looper's queue.
    pub fn post_delayed_to_main(&mut self, handler_id: u32, what: i32, delay_ms: u64) -> u32 {
        if let Some(looper) = self.main_looper_mut() {
            looper.queue.enqueue_delayed(handler_id, what, delay_ms)
        } else {
            0
        }
    }

    /// Ticks all loopers, returning (looper_id, messages) pairs for non-empty results.
    pub fn tick_all(&mut self) -> Vec<(u32, Vec<Message>)> {
        self.loopers.iter_mut()
            .map(|l| (l.looper_id, l.tick()))
            .filter(|(_, msgs)| !msgs.is_empty())
            .collect()
    }

    pub fn diagnostic_summary(&self) -> String {
        let total_pending: usize = self.loopers.iter().map(|l| l.queue.pending_count()).sum();
        format!(
            "LooperCoordinator: {} loopers, {} handlers, {} pending messages",
            self.loopers.len(),
            self.handlers.len(),
            total_pending,
        )
    }
}


/// A Runnable posted by guest code and ready for dispatch on the main runtime pump.
#[derive(Debug, Clone)]
pub struct PendingRunnable {
    pub handler_id: u32,
    pub runnable_id: u32,
    pub delivery_at: Instant,
}
pub type RunnableQueue = Arc<Mutex<Vec<PendingRunnable>>>;

fn enqueue_runnable(
    env: &crate::jni::JniEnv<'_>, args: &[Value], queue: &RunnableQueue, delay_ms: u64,
) -> Result<Option<Value>, String> {
    let handler = args.first().and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or("Handler.post receiver is null")?;
    let runnable = args.get(1).and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or("Handler.post Runnable is null")?;
    if !env.heap.contains(handler) || !env.heap.contains(runnable) {
        return Err("Handler.post received an invalid managed object".into());
    }
    queue.lock().map_err(|_| "Runnable queue lock poisoned")?.push(PendingRunnable {
        handler_id: handler, runnable_id: runnable,
        delivery_at: Instant::now() + Duration::from_millis(delay_ms),
    });
    Ok(Some(Value::Int(1)))
}

pub fn take_due_runnables(queue: &RunnableQueue, limit: usize) -> Result<Vec<PendingRunnable>, String> {
    let now = Instant::now();
    let mut pending = queue.lock().map_err(|_| "Runnable queue lock poisoned")?;
    let mut due = Vec::new();
    let mut future = Vec::with_capacity(pending.len());
    for callback in pending.drain(..) {
        if callback.delivery_at <= now && due.len() < limit { due.push(callback); }
        else { future.push(callback); }
    }
    *pending = future;
    due.sort_by_key(|callback| callback.delivery_at);
    Ok(due)
}

/// Shared identity for the guest process's main Looper object.
pub type MainLooperRef = Arc<Mutex<Option<u32>>>;

fn get_or_create_main_looper(
    env: &mut crate::jni::JniEnv<'_>,
    shared: &MainLooperRef,
) -> Result<u32, String> {
    let mut stored = shared.lock().map_err(|_| "main Looper reference lock poisoned")?;
    if let Some(id) = *stored {
        if env.heap.get(id).is_ok() {
            return Ok(id);
        }
    }
    let id = env.heap.allocate_instance("Landroid/os/Looper;")
        .map_err(|error| format!("main Looper allocation failed: {error:?}"))?;
    env.set_object_field(id, "mStrideLooperId", Value::Int(1))?;
    *stored = Some(id);
    Ok(id)
}

fn get_or_create_main_thread(
    env: &mut crate::jni::JniEnv<'_>,
    shared: &MainLooperRef,
) -> Result<u32, String> {
    let looper_id = get_or_create_main_looper(env, shared)?;
    if let Value::Object(thread_id) = env.get_object_field(looper_id, "mThread")? {
        if env.heap.contains(thread_id) {
            return Ok(thread_id);
        }
    }
    let thread_id = env.heap.allocate_instance("Ljava/lang/Thread;")
        .map_err(|error| format!("main Thread allocation failed: {error:?}"))?;
    env.set_object_field(thread_id, "mStrideThreadId", Value::Long(1))?;
    env.set_object_field(thread_id, "mStrideAlive", Value::Int(1))?;
    env.set_object_field(looper_id, "mThread", Value::Object(thread_id))?;
    Ok(thread_id)
}

fn get_or_create_looper_thread(
    env: &mut crate::jni::JniEnv<'_>,
    looper_id: u32,
) -> Result<u32, String> {
    let existing = env.get_object_field(looper_id, "mThread")?;
    if let Some(id) = existing.as_object().filter(|id| *id != 0) {
        if env.heap.get(id).is_ok() {
            return Ok(id);
        }
    }
    let thread = env.heap.allocate_instance("Ljava/lang/Thread;")
        .map_err(|error| format!("guest Thread allocation failed: {error:?}"))?;
    // This runtime currently models a single managed UI thread. Give every
    // Looper-created thread object a deterministic identity and keep the main
    // Looper/currentThread object stable across calls.
    env.set_object_field(thread, "mStrideThreadId", Value::Long(1))?;
    env.set_object_field(thread, "mStrideAlive", Value::Int(1))?;
    env.set_object_field(looper_id, "mThread", Value::Object(thread))?;
    Ok(thread)
}

/// Registers correctly typed Looper object-returning methods and Handler
/// constructors. Scheduling remains owned by the runtime event pump.
pub fn register_looper_jni(registry: &mut JniNativeRegistry, main: MainLooperRef, runnables: RunnableQueue) {
    for name in ["getMainLooper", "myLooper"] {
        let shared = main.clone();
        registry.register(
            "Landroid/os/Looper;",
            name,
            "()Landroid/os/Looper;",
            move |env, _args| {
                let id = get_or_create_main_looper(env, &shared)?;
                Ok(Some(Value::Object(id)))
            },
        );
    }

    // AndroidX ArchTaskExecutor compares Looper.getThread() with
    // Thread.currentThread(). The guest runtime currently owns one managed
    // main thread, so both APIs return the same stable guest Thread object.
    registry.register(
        "Landroid/os/Looper;",
        "getThread",
        "()Ljava/lang/Thread;",
        |env, args| {
            let looper = args.first().and_then(Value::as_object)
                .filter(|id| *id != 0).ok_or("Looper.getThread receiver is null")?;
            if env.heap.get(looper).map_err(|error| format!("Looper lookup failed: {error:?}"))?
                .class_descriptor() != "Landroid/os/Looper;" {
                return Err("Looper.getThread receiver is not an android.os.Looper".into());
            }
            let thread = get_or_create_looper_thread(env, looper)?;
            Ok(Some(Value::Object(thread)))
        },
    );

    let shared = main.clone();
    registry.register(
        "Ljava/lang/Thread;",
        "currentThread",
        "()Ljava/lang/Thread;",
        move |env, _args| {
            let thread_id = get_or_create_main_thread(env, &shared)?;
            Ok(Some(Value::Object(thread_id)))
        },
    );
    registry.register("Ljava/lang/Thread;", "getId", "()J", |env, args| {
        let thread_id = args.first().and_then(Value::as_object)
            .filter(|id| *id != 0).ok_or("Thread.getId receiver is null")?;
        let id = env.get_object_field(thread_id, "mStrideThreadId")?;
        Ok(Some(Value::Long(match id {
            Value::Long(value) => value,
            Value::Int(value) => i64::from(value),
            _ => 0,
        })))
    });
    registry.register("Ljava/lang/Thread;", "isAlive", "()Z", |env, args| {
        let thread_id = args.first().and_then(Value::as_object)
            .filter(|id| *id != 0).ok_or("Thread.isAlive receiver is null")?;
        env.heap.get(thread_id)
            .map_err(|error| format!("Thread.isAlive receiver is invalid: {error:?}"))?;
        let alive = env.get_object_field(thread_id, "mStrideAlive")?.as_int();
        Ok(Some(Value::Int(i32::from(alive != 0))))
    });

    let shared = main.clone();
    registry.register("Landroid/os/Handler;", "<init>", "()V", move |env, args| {
        let handler = args.first().and_then(Value::as_object)
            .filter(|id| *id != 0).ok_or("Handler constructor receiver is null")?;
        let looper = get_or_create_main_looper(env, &shared)?;
        env.set_object_field(handler, "mLooper", Value::Object(looper))?;
        Ok(None)
    });

    registry.register(
        "Landroid/os/Handler;",
        "<init>",
        "(Landroid/os/Looper;)V",
        |env, args| {
            let handler = args.first().and_then(Value::as_object)
                .filter(|id| *id != 0).ok_or("Handler constructor receiver is null")?;
            let looper = args.get(1).and_then(Value::as_object)
                .filter(|id| *id != 0).ok_or("Handler constructor Looper is null")?;
            if !env.heap.contains(looper) {
                return Err("Handler constructor received an invalid Looper object".into());
            }
            env.set_object_field(handler, "mLooper", Value::Object(looper))?;
            Ok(None)
        },
    );

    let queue = runnables.clone();
    registry.register("Landroid/os/Handler;", "post", "(Ljava/lang/Runnable;)Z",
        move |env, args| enqueue_runnable(env, args, &queue, 0));
    let queue = runnables.clone();
    registry.register("Landroid/os/Handler;", "postDelayed", "(Ljava/lang/Runnable;J)Z",
        move |env, args| {
            let delay = args.get(2).map(Value::as_long).unwrap_or(0).clamp(0, 86_400_000) as u64;
            enqueue_runnable(env, args, &queue, delay)
        });
    let queue = runnables.clone();
    registry.register("Landroid/os/Handler;", "removeCallbacks", "(Ljava/lang/Runnable;)V",
        move |_env, args| {
            let handler = args.first().and_then(Value::as_object).filter(|id| *id != 0)
                .ok_or("Handler.removeCallbacks receiver is null")?;
            let runnable = args.get(1).and_then(Value::as_object).filter(|id| *id != 0)
                .ok_or("Handler.removeCallbacks Runnable is null")?;
            queue.lock().map_err(|_| "Runnable queue lock poisoned")?
                .retain(|item| item.handler_id != handler || item.runnable_id != runnable);
            Ok(None)
        });

    registry.register(
        "Landroid/os/Handler;",
        "getLooper",
        "()Landroid/os/Looper;",
        |env, args| {
            let handler = args.first().and_then(Value::as_object)
                .filter(|id| *id != 0).ok_or("Handler.getLooper receiver is null")?;
            Ok(Some(env.get_object_field(handler, "mLooper")?))
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;
    use crate::jni::JniEnv;

    #[test]
    fn main_looper_jni_returns_a_stable_object_reference() {
        let mut registry = JniNativeRegistry::new();
        register_looper_jni(&mut registry, Arc::new(Mutex::new(None)), Arc::new(Mutex::new(Vec::new())));
        let mut heap = ManagedHeap::default();
        let main = registry.find("Landroid/os/Looper;", "getMainLooper",
            "()Landroid/os/Looper;").unwrap().clone();
        let first = main(&mut JniEnv::new(&mut heap), &[]).unwrap().unwrap().as_object().unwrap();
        let second = main(&mut JniEnv::new(&mut heap), &[]).unwrap().unwrap().as_object().unwrap();
        assert_eq!(first, second);
        assert_eq!(heap.get(first).unwrap().class_descriptor(), "Landroid/os/Looper;");
    }

    #[test]
    fn looper_get_thread_and_thread_current_thread_share_stable_guest_thread() {
        let mut registry = JniNativeRegistry::new();
        register_looper_jni(&mut registry, Arc::new(Mutex::new(None)), Arc::new(Mutex::new(Vec::new())));
        let mut heap = ManagedHeap::default();
        let looper = registry.find("Landroid/os/Looper;", "getMainLooper",
            "()Landroid/os/Looper;").unwrap().clone()(&mut JniEnv::new(&mut heap), &[])
            .unwrap().unwrap().as_object().unwrap();
        let get_thread = registry.find("Landroid/os/Looper;", "getThread",
            "()Ljava/lang/Thread;").unwrap().clone();
        let looper_thread = get_thread(&mut JniEnv::new(&mut heap), &[Value::Object(looper)])
            .unwrap().unwrap().as_object().unwrap();
        let current_thread = registry.find("Ljava/lang/Thread;", "currentThread",
            "()Ljava/lang/Thread;").unwrap().clone();
        let current = current_thread(&mut JniEnv::new(&mut heap), &[])
            .unwrap().unwrap().as_object().unwrap();
        let again = get_thread(&mut JniEnv::new(&mut heap), &[Value::Object(looper)])
            .unwrap().unwrap().as_object().unwrap();
        assert_eq!(looper_thread, current);
        assert_eq!(looper_thread, again);
        assert_eq!(heap.get(looper_thread).unwrap().class_descriptor(), "Ljava/lang/Thread;");
    }

    #[test]
    fn thread_is_alive_distinguishes_unstarted_and_runtime_main_threads() {
        let mut registry = JniNativeRegistry::new();
        register_looper_jni(&mut registry, Arc::new(Mutex::new(None)), Arc::new(Mutex::new(Vec::new())));
        let mut heap = ManagedHeap::default();
        let unstarted = heap.allocate_instance("Ljava/lang/Thread;").unwrap();
        let is_alive = registry.find("Ljava/lang/Thread;", "isAlive", "()Z").unwrap().clone();
        assert_eq!(
            is_alive(&mut JniEnv::new(&mut heap), &[Value::Object(unstarted)]).unwrap(),
            Some(Value::Int(0))
        );
        let current = registry.find("Ljava/lang/Thread;", "currentThread", "()Ljava/lang/Thread;")
            .unwrap().clone()(&mut JniEnv::new(&mut heap), &[]).unwrap().unwrap().as_object().unwrap();
        assert_eq!(
            is_alive(&mut JniEnv::new(&mut heap), &[Value::Object(current)]).unwrap(),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn handler_constructor_retains_main_looper_object() {
        let mut registry = JniNativeRegistry::new();
        register_looper_jni(&mut registry, Arc::new(Mutex::new(None)), Arc::new(Mutex::new(Vec::new())));
        let mut heap = ManagedHeap::default();
        let handler = heap.allocate_instance("Landroid/os/Handler;").unwrap();
        let init = registry.find("Landroid/os/Handler;", "<init>", "()V").unwrap().clone();
        init(&mut JniEnv::new(&mut heap), &[Value::Object(handler)]).unwrap();
        let get = registry.find("Landroid/os/Handler;", "getLooper",
            "()Landroid/os/Looper;").unwrap().clone();
        let looper = get(&mut JniEnv::new(&mut heap), &[Value::Object(handler)])
            .unwrap().unwrap().as_object().unwrap();
        assert_eq!(heap.get(looper).unwrap().class_descriptor(), "Landroid/os/Looper;");
    }

    #[test]
    fn handler_post_enqueues_and_remove_callbacks_cancels() {
        let mut registry = JniNativeRegistry::new();
        let queue = Arc::new(Mutex::new(Vec::new()));
        register_looper_jni(&mut registry, Arc::new(Mutex::new(None)), queue.clone());
        let mut heap = ManagedHeap::default();
        let handler = heap.allocate_instance("Landroid/os/Handler;").unwrap();
        let runnable = heap.allocate_instance("Ltest/Callback;").unwrap();
        let post = registry.find("Landroid/os/Handler;", "post", "(Ljava/lang/Runnable;)Z").unwrap().clone();
        assert_eq!(post(&mut JniEnv::new(&mut heap), &[Value::Object(handler), Value::Object(runnable)]).unwrap(), Some(Value::Int(1)));
        assert_eq!(queue.lock().unwrap().len(), 1);
        let remove = registry.find("Landroid/os/Handler;", "removeCallbacks", "(Ljava/lang/Runnable;)V").unwrap().clone();
        remove(&mut JniEnv::new(&mut heap), &[Value::Object(handler), Value::Object(runnable)]).unwrap();
        assert!(queue.lock().unwrap().is_empty());
    }

    #[test]
    fn runnable_queue_delivers_due_callbacks_and_retains_delayed_ones() {
        let queue = Arc::new(Mutex::new(Vec::new()));
        let now = Instant::now();
        queue.lock().unwrap().extend([
            PendingRunnable { handler_id: 1, runnable_id: 11, delivery_at: now + Duration::from_secs(60) },
            PendingRunnable { handler_id: 1, runnable_id: 12, delivery_at: now },
        ]);
        let due = take_due_runnables(&queue, 8).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].runnable_id, 12);
        assert_eq!(queue.lock().unwrap().len(), 1);
    }

    #[test]
    fn message_queue_enqueue_and_drain() {
        let mut q = MessageQueue::new();
        q.enqueue(1, 42);
        let due = q.drain_due();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].what, 42);
    }

    #[test]
    fn delayed_message_not_due_immediately() {
        let mut q = MessageQueue::new();
        q.enqueue_delayed(1, 99, 60_000); // 60 seconds
        let due = q.drain_due();
        assert!(due.is_empty());
    }

    #[test]
    fn remove_callbacks_works() {
        let mut q = MessageQueue::new();
        let cid = q.enqueue_delayed(1, 10, 60_000);
        assert_eq!(q.pending_count(), 1);
        q.remove_callbacks(cid);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn looper_coordinator_has_main() {
        let mut coord = LooperCoordinator::new();
        assert!(coord.main_looper_id.is_some());
        coord.post_to_main(1, 100);
        let results = coord.tick_all();
        assert!(!results.is_empty());
        assert_eq!(results[0].1[0].what, 100);
    }

    #[test]
    fn multiple_handlers_on_main_looper() {
        let mut coord = LooperCoordinator::new();
        let main_id = coord.main_looper_id.unwrap();
        let _h1 = coord.create_handler(main_id);
        let _h2 = coord.create_handler(main_id);
        assert_eq!(coord.handlers.len(), 2);
        coord.post_to_main(1, 1);
        coord.post_to_main(2, 2);
        let results = coord.tick_all();
        assert_eq!(results[0].1.len(), 2);
    }
    #[test]
    fn looper_get_thread_matches_thread_current_thread() {
        let mut registry = JniNativeRegistry::new();
        register_looper_jni(
            &mut registry,
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(Vec::new())),
        );
        let mut heap = ManagedHeap::default();
        let get_main = registry.find(
            "Landroid/os/Looper;", "getMainLooper", "()Landroid/os/Looper;"
        ).unwrap().clone();
        let looper = get_main(
            &mut crate::jni::JniEnv::new(&mut heap), &[]
        ).unwrap().and_then(|value| value.as_object()).unwrap();
        let get_thread = registry.find(
            "Landroid/os/Looper;", "getThread", "()Ljava/lang/Thread;"
        ).unwrap().clone();
        let looper_thread = get_thread(
            &mut crate::jni::JniEnv::new(&mut heap), &[Value::Object(looper)]
        ).unwrap().and_then(|value| value.as_object()).unwrap();
        let current_thread = registry.find(
            "Ljava/lang/Thread;", "currentThread", "()Ljava/lang/Thread;"
        ).unwrap().clone();
        let current = current_thread(
            &mut crate::jni::JniEnv::new(&mut heap), &[]
        ).unwrap().and_then(|value| value.as_object()).unwrap();
        assert_eq!(looper_thread, current);
        assert_eq!(heap.get(current).unwrap().class_descriptor(), "Ljava/lang/Thread;");
    }


}
