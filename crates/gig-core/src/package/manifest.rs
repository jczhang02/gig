//! `<id>.manifest.toml`: the explicit allowlist.

use crate::models::PackageKind;
use crate::package::rules;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: i64,
    pub package_id: String,
    pub kind: PackageKind,
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_named: Vec<String>,
}

impl Manifest {
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::NotFound(format!("manifest {} ({e})", path.display())))?;
        let m: Manifest = toml::from_str(&text)?;
        Ok(m)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text).map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))
    }

    /// Structural checks independent of the filesystem. Returns naming waivers as
    /// (path, prefix) pairs for the warnings list.
    pub fn validate(&self, expected_id: &str) -> Result<Vec<(String, String)>> {
        if self.version != 1 {
            return Err(Error::UnsafePackage(format!(
                "unsupported manifest version {}",
                self.version
            )));
        }
        if self.package_id != expected_id {
            return Err(Error::UnsafePackage(format!(
                "manifest package_id {:?} does not match {expected_id:?}",
                self.package_id
            )));
        }
        if self.files.is_empty() {
            return Err(Error::UnsafePackage("manifest lists no files".into()));
        }
        for prefix in &self.client_named {
            if !prefix.ends_with('/') {
                return Err(Error::UnsafePackage(format!(
                    "client_named prefix {prefix:?} must end with '/'"
                )));
            }
            rules::hard(prefix.trim_end_matches('/'))?;
        }
        let mut seen = HashSet::new();
        let mut waived = Vec::new();
        for f in &self.files {
            if !seen.insert(f.as_str()) {
                return Err(Error::UnsafePackage(format!(
                    "duplicate manifest entry {f:?}"
                )));
            }
            if let rules::Convention::Exempted(prefix) = rules::check_path(f, &self.client_named)? {
                waived.push((f.clone(), prefix));
            }
        }
        Ok(waived)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(files: &[&str]) -> Manifest {
        Manifest {
            version: 1,
            package_id: "p-v1".into(),
            kind: PackageKind::Full,
            files: files.iter().map(|s| s.to_string()).collect(),
            client_named: vec![],
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p-v1.manifest.toml");
        let mut original = m(&["a.pdf", "r/x.png"]);
        original.client_named = vec!["r/".into()];
        original.write(&p).unwrap();
        let back = Manifest::read(&p).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn structural_rules() {
        assert!(m(&[]).validate("p-v1").is_err());
        assert!(m(&["a", "a"]).validate("p-v1").is_err());
        assert!(m(&["a"]).validate("other").is_err());
        assert!(m(&["../a"]).validate("p-v1").is_err());
        assert!(m(&["a"]).validate("p-v1").unwrap().is_empty());
        let mut w = m(&["r/图.png"]);
        assert!(w.validate("p-v1").is_err());
        w.client_named = vec!["r/".into()];
        assert_eq!(
            w.validate("p-v1").unwrap(),
            vec![("r/图.png".to_string(), "r/".to_string())]
        );
        w.client_named = vec!["r".into()];
        assert!(w.validate("p-v1").is_err());
    }

    #[test]
    fn unknown_fields_rejected() {
        let text = "version = 1\npackage_id = \"p\"\nkind = \"full\"\nfiles = [\"a\"]\nextra = 1\n";
        assert!(toml::from_str::<Manifest>(text).is_err());
    }
}
