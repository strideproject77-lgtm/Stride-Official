//! Object monitor synchronization for Dalvik monitor-enter and monitor-exit.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    NullPointer,
    IllegalMonitorState { object_id: u32, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Monitor {
    owner_thread: u64,
    recursion_count: u32,
}

/// Managed object synchronization monitor table.
/// Provides re-entrant locking per object ID.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MonitorTable {
    locks: HashMap<u32, Monitor>,
}

impl MonitorTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn monitor_enter(&mut self, object_id: u32, thread_id: u64) -> Result<(), SyncError> {
        if object_id == 0 {
            return Err(SyncError::NullPointer);
        }
        if let Some(monitor) = self.locks.get_mut(&object_id) {
            if monitor.owner_thread == thread_id {
                monitor.recursion_count = monitor.recursion_count.saturating_add(1);
                Ok(())
            } else {
                // If contended, in this synchronous thread execution model we reject or block
                Err(SyncError::IllegalMonitorState {
                    object_id,
                    message: format!("Object {} is held by another thread ({})", object_id, monitor.owner_thread),
                })
            }
        } else {
            self.locks.insert(
                object_id,
                Monitor {
                    owner_thread: thread_id,
                    recursion_count: 1,
                },
            );
            Ok(())
        }
    }

    pub fn monitor_exit(&mut self, object_id: u32, thread_id: u64) -> Result<(), SyncError> {
        if object_id == 0 {
            return Err(SyncError::NullPointer);
        }
        let remove = if let Some(monitor) = self.locks.get_mut(&object_id) {
            if monitor.owner_thread != thread_id {
                return Err(SyncError::IllegalMonitorState {
                    object_id,
                    message: format!("Thread {} does not hold lock for object {}", thread_id, object_id),
                });
            }
            monitor.recursion_count = monitor.recursion_count.saturating_sub(1);
            monitor.recursion_count == 0
        } else {
            return Err(SyncError::IllegalMonitorState {
                object_id,
                message: format!("Object {} is not locked", object_id),
            });
        };

        if remove {
            self.locks.remove(&object_id);
        }
        Ok(())
    }

    pub fn is_locked(&self, object_id: u32) -> bool {
        self.locks.contains_key(&object_id)
    }

    pub fn lock_owner(&self, object_id: u32) -> Option<u64> {
        self.locks.get(&object_id).map(|m| m.owner_thread)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_enter_exit_and_recursion() {
        let mut monitors = MonitorTable::new();
        monitors.monitor_enter(42, 1).unwrap();
        assert!(monitors.is_locked(42));
        assert_eq!(monitors.lock_owner(42), Some(1));

        // Re-entrant lock on same thread
        monitors.monitor_enter(42, 1).unwrap();

        // Release first recursion
        monitors.monitor_exit(42, 1).unwrap();
        assert!(monitors.is_locked(42));

        // Release final recursion
        monitors.monitor_exit(42, 1).unwrap();
        assert!(!monitors.is_locked(42));
    }

    #[test]
    fn monitor_rejects_unowned_exit() {
        let mut monitors = MonitorTable::new();
        assert!(matches!(
            monitors.monitor_exit(10, 1),
            Err(SyncError::IllegalMonitorState { .. })
        ));
    }
}
