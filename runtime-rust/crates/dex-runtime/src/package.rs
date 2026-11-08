//! Android Package and XAPK / OBB Expansion File Manager.
//!
//! Handles complex Android distribution packages including:
//! - Standard APKs
//! - XAPK bundles with split APKs (ABI splits, density splits)
//! - OBB expansion files (main.<version>.<package>.obb, patch.<version>.<package>.obb).

use std::collections::HashMap;

/// An Android OBB (Opaque Binary Blob) expansion file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObbFile {
    pub file_name: String,
    pub is_main: bool,
    pub version_code: u32,
    pub package_name: String,
    pub size_bytes: usize,
}

impl ObbFile {
    /// Parses an OBB filename like "main.1024.com.example.game.obb" or "patch.1024.com.example.game.obb".
    pub fn parse_filename(name: &str, size_bytes: usize) -> Option<Self> {
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() < 4 {
            return None;
        }

        let is_main = match parts[0] {
            "main" => true,
            "patch" => false,
            _ => return None,
        };

        let version_code = parts[1].parse::<u32>().ok()?;
        let last_part = parts.last()?;
        if *last_part != "obb" {
            return None;
        }

        // Package name is joined from parts[2..parts.len()-1]
        let pkg = parts[2..parts.len() - 1].join(".");

        Some(Self {
            file_name: name.to_string(),
            is_main,
            version_code,
            package_name: pkg,
            size_bytes,
        })
    }
}

/// A parsed XAPK container containing the base APK, splits, and OBB files.
#[derive(Debug, Clone, Default)]
pub struct XapkBundle {
    pub package_name: String,
    pub version_code: u32,
    pub base_apk_name: Option<String>,
    pub split_apk_names: Vec<String>,
    pub obb_files: Vec<ObbFile>,
}

impl XapkBundle {
    pub fn new(package_name: &str, version_code: u32) -> Self {
        Self {
            package_name: package_name.to_string(),
            version_code,
            base_apk_name: None,
            split_apk_names: Vec::new(),
            obb_files: Vec::new(),
        }
    }

    /// Detects file role within an XAPK zip inventory.
    pub fn register_entry(&mut self, entry_name: &str, size: usize) {
        if entry_name.ends_with(".obb") {
            if let Some(obb) = ObbFile::parse_filename(entry_name, size) {
                self.obb_files.push(obb);
            }
        } else if entry_name.ends_with(".apk") {
            if entry_name.contains("base") || self.base_apk_name.is_none() && !entry_name.starts_with("config.") {
                self.base_apk_name = Some(entry_name.to_string());
            } else {
                self.split_apk_names.push(entry_name.to_string());
            }
        }
    }

    /// Returns whether this package contains split APKs or OBB expansions.
    pub fn is_multi_component(&self) -> bool {
        !self.split_apk_names.is_empty() || !self.obb_files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_obb_filenames() {
        let obb1 = ObbFile::parse_filename("main.2048.com.stride.racing.obb", 1024 * 1024).unwrap();
        assert!(obb1.is_main);
        assert_eq!(obb1.version_code, 2048);
        assert_eq!(obb1.package_name, "com.stride.racing");
        assert_eq!(obb1.size_bytes, 1024 * 1024);

        let patch = ObbFile::parse_filename("patch.2048.com.stride.racing.obb", 512).unwrap();
        assert!(!patch.is_main);
        assert_eq!(patch.version_code, 2048);
        assert_eq!(patch.package_name, "com.stride.racing");

        assert!(ObbFile::parse_filename("invalid.txt", 100).is_none());
    }

    #[test]
    fn xapk_bundle_assembly() {
        let mut bundle = XapkBundle::new("com.stride.racing", 2048);
        bundle.register_entry("com.stride.racing.apk", 15_000_000);
        bundle.register_entry("config.arm64_v8a.apk", 8_000_000);
        bundle.register_entry("config.xxhdpi.apk", 12_000_000);
        bundle.register_entry("main.2048.com.stride.racing.obb", 500_000_000);

        assert_eq!(bundle.base_apk_name.as_deref(), Some("com.stride.racing.apk"));
        assert_eq!(bundle.split_apk_names.len(), 2);
        assert_eq!(bundle.obb_files.len(), 1);
        assert!(bundle.is_multi_component());
    }
}
