//! Own private atomic state and response artifact files.
pub mod globals;

use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
};

pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Atomic globals writes update the directory timestamp. The editor watches
    /// this cheap revision so a separate request process can refresh diagnostics.
    pub fn globals_revision(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(self.root.join("globals"))
            .ok()?
            .modified()
            .ok()
    }

    pub fn new() -> Result<Self> {
        let root = if let Some(root) = std::env::var_os("SENDLINE_HOME") {
            PathBuf::from(root)
        } else {
            let home = std::env::var_os("HOME").context("Cannot find your home directory")?;
            #[cfg(target_os = "macos")]
            let root = PathBuf::from(home).join("Library/Caches/sendline");
            #[cfg(not(target_os = "macos"))]
            let root = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(home).join(".cache"))
                .join("sendline");
            root
        };
        Self::open(root)
    }

    pub fn open(root: PathBuf) -> Result<Self> {
        private_directory(&root)?;
        private_directory(&root.join("environments"))?;
        private_directory(&root.join("responses"))?;
        private_directory(&root.join("globals"))?;
        Ok(Self { root })
    }

    pub fn globals_path(&self, directory: &Path, environment: Option<&str>) -> PathBuf {
        let mut hash = DefaultHasher::new();
        (directory, environment).hash(&mut hash);
        self.root
            .join("globals")
            .join(format!("{:016x}.json", hash.finish()))
    }

    pub fn console_path(&self, source: &Path) -> PathBuf {
        let mut hash = DefaultHasher::new();
        source.hash(&mut hash);
        self.response_path(
            &format!(
                "console-{:016x}-{}",
                hash.finish(),
                uuid::Uuid::new_v4().simple()
            ),
            "md",
        )
    }

    fn selection_path(&self, directory: &Path) -> PathBuf {
        let mut hash = DefaultHasher::new();
        directory.hash(&mut hash);
        self.root
            .join("environments")
            .join(format!("{:016x}.json", hash.finish()))
    }

    pub fn selection(&self, directory: &Path) -> Result<Option<String>> {
        let path = self.selection_path(directory);
        match std::fs::read(path) {
            Ok(bytes) => {
                let value: Value =
                    serde_json::from_slice(&bytes).context("Cannot read saved HTTP environment")?;
                // Check the original key as well as its filename hash.
                if value["directory"].as_str() != directory.to_str() {
                    return Ok(None);
                }
                Ok(value["environment"].as_str().map(str::to_string))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn select(&self, directory: &Path, name: &str) -> Result<()> {
        self.write(
            &self.selection_path(directory),
            &serde_json::to_vec(&serde_json::json!({
                "directory": directory, "environment": name
            }))?,
        )
    }

    pub fn response_path(&self, stem: &str, extension: &str) -> PathBuf {
        self.root
            .join("responses")
            .join(format!("{stem}.{extension}"))
    }

    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().context("Invalid response path")?)?;
        temporary.write_all(bytes)?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

fn private_directory(path: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
