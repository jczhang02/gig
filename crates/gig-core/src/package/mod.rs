//! Client packages: the allowlist manifest, the path rules, the zip, and the check.
//!
//! Layout (fixed):
//!   <dev_path>/delivery/<id>/                 client files only
//!   <dev_path>/delivery/<id>.manifest.toml    allowlist, outside the zip
//!   <dev_path>/delivery/<id>.zip              entries == manifest.files

pub mod build;
pub mod manifest;
pub mod rules;
pub mod validate;

use std::path::{Path, PathBuf};

pub const DELIVERY_DIR: &str = "delivery";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub delivery_dir: PathBuf,
    pub package_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub zip_path: PathBuf,
}

impl Layout {
    pub fn new(dev_path: &Path, package_id: &str) -> Self {
        let delivery_dir = dev_path.join(DELIVERY_DIR);
        Self {
            package_dir: delivery_dir.join(package_id),
            manifest_path: delivery_dir.join(format!("{package_id}.manifest.toml")),
            zip_path: delivery_dir.join(format!("{package_id}.zip")),
            delivery_dir,
        }
    }
}
