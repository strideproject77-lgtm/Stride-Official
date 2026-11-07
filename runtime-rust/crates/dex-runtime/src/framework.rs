//! Android framework integration boundaries: Activity lifecycle, Context, and SharedPreferences.

use std::collections::HashMap;
use crate::types::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLifecycleState {
    NotStarted,
    Created,
    Started,
    Resumed,
    Paused,
    Stopped,
    Destroyed,
}

/// Simulated Android SharedPreferences storage.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SharedPreferences {
    values: HashMap<String, Value>,
}

impl SharedPreferences {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_int(&mut self, key: &str, value: i32) {
        self.values.insert(key.to_owned(), Value::Int(value));
    }

    pub fn get_int(&self, key: &str, default: i32) -> i32 {
        self.values.get(key).map(|v| v.as_int()).unwrap_or(default)
    }

    pub fn put_long(&mut self, key: &str, value: i64) {
        self.values.insert(key.to_owned(), Value::Long(value));
    }

    pub fn get_long(&self, key: &str, default: i64) -> i64 {
        self.values.get(key).map(|v| v.as_long()).unwrap_or(default)
    }

    pub fn put_string(&mut self, key: &str, string_id: u32) {
        self.values.insert(key.to_owned(), Value::Object(string_id));
    }

    pub fn get_string(&self, key: &str) -> Option<u32> {
        self.values.get(key).and_then(|v| v.as_object())
    }
}

/// Android execution environment hosting packages, preferences, and diagnostics.
#[derive(Debug, Default, Clone)]
pub struct AndroidEnvironment {
    pub package_name: String,
    pub shared_preferences: HashMap<String, SharedPreferences>,
    pub activity_state: HashMap<String, ActivityLifecycleState>,
    pub logs: Vec<String>,
}

impl AndroidEnvironment {
    pub fn new(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_owned(),
            shared_preferences: HashMap::new(),
            activity_state: HashMap::new(),
            logs: Vec::new(),
        }
    }

    pub fn get_preferences(&mut self, name: &str) -> &mut SharedPreferences {
        self.shared_preferences.entry(name.to_owned()).or_default()
    }

    pub fn log(&mut self, tag: &str, message: &str) {
        self.logs.push(format!("{}: {}", tag, message));
    }

    pub fn transition_activity(&mut self, class_descriptor: &str, new_state: ActivityLifecycleState) {
        self.activity_state.insert(class_descriptor.to_owned(), new_state);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_preferences_roundtrip() {
        let mut env = AndroidEnvironment::new("com.example.game");
        let prefs = env.get_preferences("settings");
        prefs.put_int("sound_volume", 80);
        prefs.put_long("high_score", 99999);

        assert_eq!(prefs.get_int("sound_volume", 0), 80);
        assert_eq!(prefs.get_long("high_score", 0), 99999);
        assert_eq!(prefs.get_int("unknown", 5), 5);
    }
}
