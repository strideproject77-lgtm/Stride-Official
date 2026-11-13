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
    pub mime_type: Option<String>,
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
            mime_type: None,
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

    /// Set a data URI and clear the MIME type, matching Android Intent semantics.
    pub fn set_data(&mut self, uri: Option<&str>) {
        self.data_uri = uri.map(str::to_owned);
        self.mime_type = None;
    }

    /// Set a MIME type and clear the data URI, matching Android Intent semantics.
    pub fn set_type(&mut self, mime_type: Option<&str>) {
        self.mime_type = mime_type.map(str::to_owned);
        self.data_uri = None;
    }

    /// Set both data URI and MIME type without clearing either value.
    pub fn set_data_and_type(&mut self, uri: Option<&str>, mime_type: Option<&str>) {
        self.data_uri = uri.map(str::to_owned);
        self.mime_type = mime_type.map(str::to_owned);
    }

    pub fn get_scheme(&self) -> Option<&str> {
        let uri = self.data_uri.as_deref()?;
        let colon = uri.find(':')?;
        let scheme = &uri[..colon];
        let mut chars = scheme.chars();
        let first = chars.next()?;
        (first.is_ascii_alphabetic()
            && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
            .then_some(scheme)
    }

    /// Android Intent.filterEquals ignores extras and flags.
    pub fn filter_equals(&self, other: &Self) -> bool {
        self.action == other.action
            && self.data_uri == other.data_uri
            && self.mime_type == other.mime_type
            && self.component_package == other.component_package
            && self.component_class == other.component_class
            && self.categories == other.categories
    }

    pub fn remove_category(&mut self, category: &str) {
        self.categories.retain(|existing| existing != category);
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

/// Explicit outcomes for the bounded, current-app PackageManager lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageManagerError {
    MetadataUnavailable,
    PackageNotFound(String),
    ManifestPackageMismatch { environment: String, manifest: String },
}

impl PackageInfo {
    /// Build the PackageManager-facing package record from parsed AndroidManifest.xml metadata.
    pub fn from_manifest(manifest: &stride_axml::ManifestMetadata) -> Result<Self, String> {
        let package_name = manifest.package_name.as_deref()
            .filter(|name| !name.is_empty())
            .ok_or("manifest package identity is required for PackageInfo")?;

        let mut activities = Vec::new();
        let mut services = Vec::new();
        for component in &manifest.components {
            match component.kind.as_str() {
                "activity" | "activity-alias" => activities.push(ActivityInfo {
                    name: component.name.clone(),
                    exported: component.exported.unwrap_or(false),
                    screen_orientation: component.screen_orientation
                        .or(manifest.screen_orientation)
                        .unwrap_or(-1),
                }),
                "service" => services.push(ServiceInfo {
                    name: component.name.clone(),
                    exported: component.exported.unwrap_or(false),
                    permission: None,
                }),
                _ => {}
            }
        }

        Ok(Self {
            package_name: package_name.to_owned(),
            version_code: manifest.version_code.unwrap_or(0),
            version_name: manifest.version_name.clone().unwrap_or_default(),
            min_sdk: manifest.min_sdk.unwrap_or(1),
            target_sdk: manifest.target_sdk.unwrap_or(1),
            permissions: manifest.permissions.clone(),
            activities,
            services,
            application_class: manifest.application_name.clone(),
            launcher_activity: manifest.launcher_activity.clone(),
        })
    }
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

    /// Attach manifest-derived package metadata to this app environment.
    /// The manifest identity must match the environment identity exactly.
    pub fn load_package_info_from_manifest(
        &mut self,
        manifest: &stride_axml::ManifestMetadata,
    ) -> Result<(), PackageManagerError> {
        let info = PackageInfo::from_manifest(manifest)
            .map_err(|_| PackageManagerError::MetadataUnavailable)?;
        if info.package_name != self.package_name {
            return Err(PackageManagerError::ManifestPackageMismatch {
                environment: self.package_name.clone(),
                manifest: info.package_name,
            });
        }
        self.package_info = Some(info);
        Ok(())
    }

    /// Bounded PackageManager.getPackageInfo-style lookup for this app only.
    /// Other packages are reported as not found; no fabricated metadata is returned.
    pub fn get_package_info(
        &self,
        requested_package: &str,
    ) -> Result<&PackageInfo, PackageManagerError> {
        if requested_package != self.package_name {
            return Err(PackageManagerError::PackageNotFound(requested_package.to_owned()));
        }
        self.package_info.as_ref().ok_or(PackageManagerError::MetadataUnavailable)
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
#[cfg(test)]
mod tests {
    use super::*;

    fn package_fixture_manifest(package_name: Option<&str>) -> stride_axml::ManifestMetadata {
        stride_axml::ManifestMetadata {
            package_name: package_name.map(str::to_owned),
            version_name: Some("1.2.3".to_owned()),
            version_code: Some(123),
            min_sdk: Some(23),
            target_sdk: Some(35),
            permissions: vec!["android.permission.INTERNET".to_owned()],
            application_name: None,
            components: Vec::new(),
            launcher_activity: None,
            screen_orientation: None,
        }
    }


    #[test]
    fn framework_intent_data_type_and_filter_comparison_follow_android_rules() {
        let mut left = Intent::new(Some(Intent::ACTION_VIEW));
        left.set_data(Some("content://com.example.provider/item/7"));
        assert_eq!(left.get_scheme(), Some("content"));
        assert_eq!(left.mime_type, None);
        left.set_type(Some("image/png"));
        assert_eq!(left.data_uri, None);
        assert_eq!(left.mime_type.as_deref(), Some("image/png"));
        left.set_data_and_type(Some("content://com.example.provider/item/7"), Some("image/png"));
        let mut right = left.clone();
        left.extras.put_int("ignored", 5);
        right.flags = 0x20;
        assert!(left.filter_equals(&right));
        right.set_data(Some("content://com.example.provider/item/8"));
        assert!(!left.filter_equals(&right));
        left.add_category("android.intent.category.DEFAULT");
        left.add_category("custom.category");
        left.remove_category("custom.category");
        assert_eq!(left.categories, vec!["android.intent.category.DEFAULT"]);
    }

    #[test]
    fn package_manager_returns_only_loaded_current_package_metadata() {
        let manifest = package_fixture_manifest(Some("com.stride.fixture"));
        let mut env = AndroidEnvironment::new("com.stride.fixture");
        assert_eq!(env.get_package_info("com.stride.fixture"), Err(PackageManagerError::MetadataUnavailable));
        env.load_package_info_from_manifest(&manifest).unwrap();
        let info = env.get_package_info("com.stride.fixture").unwrap();
        assert_eq!(info.version_name, "1.2.3");
        assert_eq!(info.version_code, 123);
        assert_eq!(env.get_package_info("com.other.app"), Err(PackageManagerError::PackageNotFound("com.other.app".to_owned())));
    }

    #[test]
    fn package_manager_rejects_manifest_identity_mismatch_without_mutating_state() {
        let mut env = AndroidEnvironment::new("com.stride.fixture");
        let mismatch = package_fixture_manifest(Some("com.attacker.other"));
        assert!(matches!(
            env.load_package_info_from_manifest(&mismatch),
            Err(PackageManagerError::ManifestPackageMismatch { .. })
        ));
        assert!(env.package_info.is_none());
        assert!(matches!(
            env.load_package_info_from_manifest(&package_fixture_manifest(None)),
            Err(PackageManagerError::MetadataUnavailable)
        ));
        assert!(env.package_info.is_none());
    }

    #[test]
    fn package_info_maps_manifest_identity_versions_permissions_and_components() {
        let manifest = stride_axml::ManifestMetadata {
            package_name: Some("com.stride.fixture".to_owned()),
            version_name: Some("2.4.1".to_owned()),
            version_code: Some(241),
            min_sdk: Some(23),
            target_sdk: Some(35),
            permissions: vec!["android.permission.INTERNET".to_owned()],
            application_name: Some("com.stride.fixture.App".to_owned()),
            components: vec![
                stride_axml::ManifestComponent {
                    kind: "activity".to_owned(),
                    name: "com.stride.fixture.MainActivity".to_owned(),
                    exported: Some(true),
                    has_launcher_intent: true,
                    screen_orientation: Some(0),
                },
                stride_axml::ManifestComponent {
                    kind: "service".to_owned(),
                    name: "com.stride.fixture.SyncService".to_owned(),
                    exported: None,
                    has_launcher_intent: false,
                    screen_orientation: None,
                },
                stride_axml::ManifestComponent {
                    kind: "receiver".to_owned(),
                    name: "com.stride.fixture.BootReceiver".to_owned(),
                    exported: Some(false),
                    has_launcher_intent: false,
                    screen_orientation: None,
                },
            ],
            launcher_activity: Some("com.stride.fixture.MainActivity".to_owned()),
            screen_orientation: Some(1),
        };

        let info = PackageInfo::from_manifest(&manifest).unwrap();
        assert_eq!(info.package_name, "com.stride.fixture");
        assert_eq!(info.version_code, 241);
        assert_eq!(info.version_name, "2.4.1");
        assert_eq!(info.min_sdk, 23);
        assert_eq!(info.target_sdk, 35);
        assert_eq!(info.permissions, vec!["android.permission.INTERNET"]);
        assert_eq!(info.application_class.as_deref(), Some("com.stride.fixture.App"));
        assert_eq!(info.launcher_activity.as_deref(), Some("com.stride.fixture.MainActivity"));
        assert_eq!(info.activities.len(), 1);
        assert!(info.activities[0].exported);
        assert_eq!(info.activities[0].screen_orientation, 0);
        assert_eq!(info.services.len(), 1);
        assert!(!info.services[0].exported);
        assert_eq!(info.services[0].permission, None);
    }

    #[test]
    fn package_info_rejects_missing_manifest_package_identity() {
        let manifest = stride_axml::ManifestMetadata {
            package_name: None,
            version_name: None,
            version_code: None,
            min_sdk: None,
            target_sdk: None,
            permissions: Vec::new(),
            application_name: None,
            components: Vec::new(),
            launcher_activity: None,
            screen_orientation: None,
        };
        assert!(matches!(
            PackageInfo::from_manifest(&manifest),
            Err(error) if error.contains("package identity is required")
        ));
    }
}
