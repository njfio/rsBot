use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub(super) const PACKAGE_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PackageManifest {
    pub(super) schema_version: u32,
    pub(super) name: String,
    pub(super) version: String,
    #[serde(default)]
    pub(super) signing_key: Option<String>,
    #[serde(default)]
    pub(super) signature_file: Option<String>,
    #[serde(default)]
    pub(super) templates: Vec<PackageComponent>,
    #[serde(default)]
    pub(super) skills: Vec<PackageComponent>,
    #[serde(default)]
    pub(super) extensions: Vec<PackageComponent>,
    #[serde(default)]
    pub(super) themes: Vec<PackageComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PackageComponent {
    pub(super) id: String,
    pub(super) path: String,
    #[serde(default)]
    pub(super) url: Option<String>,
    #[serde(default)]
    pub(super) sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PackageActivationSelection {
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) owner: String,
    pub(super) source: PathBuf,
}
