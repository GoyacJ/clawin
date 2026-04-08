#![forbid(unsafe_code)]

//! Configuration path and store placeholders for the Phase 1 skeleton.

use std::path::{Path, PathBuf};

/// Clawin global namespace directory name.
pub const GLOBAL_NAMESPACE_DIR_NAME: &str = ".clawin";

/// Clawin project-local metadata directory name.
pub const PROJECT_DIRECTORY_NAME: &str = ".clawin";

/// Clawin project-local instruction manifest.
pub const PROJECT_MANIFEST_NAME: &str = "CLAWIN.md";

/// Minimal path bundle for the Clawin namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClawinPaths {
    global_root: PathBuf,
    project_root: PathBuf,
}

impl ClawinPaths {
    /// Create a new path bundle for the current workspace.
    pub fn new(global_root: PathBuf, project_root: PathBuf) -> Self {
        Self {
            global_root,
            project_root,
        }
    }

    /// Borrow the global configuration root.
    pub fn global_root(&self) -> &Path {
        &self.global_root
    }

    /// Borrow the canonical project root placeholder.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Resolve the project-local `.clawin/` directory.
    pub fn project_directory(&self) -> PathBuf {
        self.project_root.join(PROJECT_DIRECTORY_NAME)
    }

    /// Resolve the project-local `CLAWIN.md` manifest.
    pub fn project_manifest(&self) -> PathBuf {
        self.project_root.join(PROJECT_MANIFEST_NAME)
    }
}

/// Minimal config store contract for later persistence work.
pub trait ConfigStore {
    /// Return the active path bundle.
    fn paths(&self) -> &ClawinPaths;

    /// Return the schema version for persistent structures.
    fn schema_version(&self) -> u32;
}

/// In-memory placeholder config store used during Phase 1.
#[derive(Clone, Debug)]
pub struct StaticConfigStore {
    paths: ClawinPaths,
    schema_version: u32,
}

impl StaticConfigStore {
    /// Create an in-memory config store with a fixed schema version.
    pub fn new(paths: ClawinPaths, schema_version: u32) -> Self {
        Self {
            paths,
            schema_version,
        }
    }
}

impl ConfigStore for StaticConfigStore {
    fn paths(&self) -> &ClawinPaths {
        &self.paths
    }

    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
