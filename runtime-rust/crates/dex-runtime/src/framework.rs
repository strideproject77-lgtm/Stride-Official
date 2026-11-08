//! Android framework integration boundaries: Context, Activity lifecycle, Intent, Bundle,
//! PackageManager, Resources, Assets, and SharedPreferences.

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

/// Type-safe bundle mapping for passing arguments between Android components.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Bundle {
    pub ints: HashMap<String, i32>,
    pub longs: HashMap<String, i64>,
    pub floats: HashMap<String, f32>,
    pub doubles: HashMap<String, f64>,
    pub booleans: HashMap<String, bool>,
    pub strings: HashMap<String, String>,
    pub byte_arrays: HashMap<String, Vec<u8>>,
}

impl Bundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_int(&mut self, key: &str, value: i32) {
        self.ints.insert(key.to_string(), value);
    }

    pub fn get_int(&self, key: &str, default: i32) -> i32 {
        self.ints.get(key).copied().unwrap_or(default)
    }

    pub fn put_string(&mut self, key: &str, value: &str) {
        self.strings.insert(key.to_string(), value.to_string());
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }

    pub fn put_boolean(&mut self, key: &str, value: bool) {
        self.booleans.insert(key.to_string(), value);
    }

    pub fn get_boolean(&self, key: &str, default: bool) -> bool {
        self.booleans.get(key).copied().unwrap_or(default)
    }
}

/// Android Intent representing a component invocation or system message.
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub action: Option<String>,
    pub categories: Vec<String>,
    pub component_package: Option<String>,
    pub component_class: Option<String>,
    pub data_uri: Option<String>,
    pub flags: u32,
    pub extras: Bundle,
}

impl Intent {
    pub const ACTION_MAIN: &'static str = "android.intent.action.MAIN";
    pub const ACTION_VIEW: &'static str = "android.intent.action.VIEW";
    pub const CATEGORY_LAUNCHER: &'static str = "android.intent.category.LAUNCHER";
    pub const CATEGORY_DEFAULT: &'static str = "android.intent.category.DEFAULT";

    pub fn new(action: Option<&str>) -> Self {
        Self {
            action: action.map(|s| s.to_string()),
            categories: Vec::new(),
            component_package: None,
            component_class: None,
            data_uri: None,
            flags: 0,
            extras: Bundle::new(),
        }
    }

    pub fn with_component(mut self, package: &str, class_name: &str) -> Self {
        self.component_package = Some(package.to_string());
        self.component_class = Some(class_name.to_string());
        self
    }

    pub fn add_category(&mut self, category: &str) {
        if !self.categories.contains(&category.to_string()) {
            self.categories.push(category.to_string());
        }
    }
}

/// Package and component metadata reported by PackageManager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityInfo {
    pub name: String,
    pub exported: bool,
    pub screen_orientation: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    pub name: String,
    pub exported: bool,
    pub permission: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub package_name: String,
    pub version_code: u32,
    pub version_name: String,
    pub min_sdk: u32,
    pub target_sdk: u32,
    pub permissions: Vec<String>,
    pub activities: Vec<ActivityInfo>,
    pub services: Vec<ServiceInfo>,
    pub application_class: Option<String>,
    pub launcher_activity: Option<String>,
}

/// AssetManager providing access to raw packaged assets.
#[derive(Debug, Default, Clone)]
pub struct AssetManager {
    pub assets: HashMap<String, Vec<u8>>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&self, filename: &str) -> Option<&[u8]> {
        self.assets.get(filename).map(|v| v.as_slice())
    }

    pub fn add_asset(&mut self, path: &str, data: Vec<u8>) {
        self.assets.insert(path.to_string(), data);
    }
}

/// Resource table abstraction providing string, color, and dimen lookups.
#[derive(Debug, Default, Clone)]
pub struct Resources {
    pub string_resources: HashMap<u32, String>,
    pub integer_resources: HashMap<u32, i32>,
    pub color_resources: HashMap<u32, u32>,
}

impl Resources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_string(&self, res_id: u32) -> Option<&str> {
        self.string_resources.get(&res_id).map(|s| s.as_str())
    }

    pub fn set_string(&mut self, res_id: u32, text: &str) {
        self.string_resources.insert(res_id, text.to_string());
    }
}

/// Android SharedPreferences storage with commit and apply support.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SharedPreferences {
    values: HashMap<String, Value>,
    string_values: HashMap<String, String>,
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

    pub fn put_string_value(&mut self, key: &str, value: &str) {
        self.string_values.insert(key.to_owned(), value.to_owned());
    }

    pub fn get_string_value(&self, key: &str, default: &str) -> String {
        self.string_values.get(key).cloned().unwrap_or_else(|| default.to_owned())
    }

    pub fn put_string(&mut self, key: &str, string_id: u32) {
        self.values.insert(key.to_owned(), Value::Object(string_id));
    }

    pub fn get_string(&self, key: &str) -> Option<u32> {
        self.values.get(key).and_then(|v| v.as_object())
    }

    /// Persist pending preference changes synchronously.
    ///
    /// The current runtime stores preferences directly in its in-memory
    /// AndroidEnvironment, so writes are already visible immediately.
    /// Returning true mirrors Android SharedPreferences.Editor.commit()
    /// while a durable on-disk backend is not yet available.
    pub fn commit(&mut self) -> bool {
        true
    }

    /// Apply preference changes asynchronously.
    ///
    /// Writes are already applied to the in-memory environment; this method
    /// exists for API compatibility with Android SharedPreferences.Editor.
    pub fn apply(&mut self) {}
}

/// Full Android execution environment hosting packages, preferences, assets, and process state.
#[derive(Debug, Default, Clone)]
pub struct AndroidEnvironment {
    pub package_name: String,
    pub package_info: Option<PackageInfo>,
    pub shared_preferences: HashMap<String, SharedPreferences>,
    pub activity_state: HashMap<String, ActivityLifecycleState>,
    pub assets: AssetManager,
    pub resources: Resources,
    pub files_dir: String,
    pub cache_dir: String,
    pub logs: Vec<String>,
}

impl AndroidEnvironment {
    pub fn new(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_owned(),
            package_info: None,
            shared_preferences: HashMap::new(),
            activity_state: HashMap::new(),
            assets: AssetManager::new(),
            resources: Resources::new(),
            files_dir: format!("/data/data/{}/files", package_name),
            cache_dir: format!("/data/data/{}/cache", package_name),
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

    pub fn get_activity_state(&self, class_descriptor: &str) -> ActivityLifecycleState {
        self.activity_state.get(class_descriptor).copied().unwrap_or(ActivityLifecycleState::NotStarted)
    }
}
