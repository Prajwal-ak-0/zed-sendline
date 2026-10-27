//! Private response-script variables, isolated by project and environment.
use crate::storage::Storage;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    path::{Path, PathBuf},
};

const MAX_GLOBAL_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Mutation {
    Set { name: String, value: Value },
    Clear { name: String },
    ClearAll,
}

#[derive(Deserialize, Serialize)]
struct SavedGlobals {
    directory: PathBuf,
    environment: Option<String>,
    values: Map<String, Value>,
}

pub struct Globals<'a> {
    storage: &'a Storage,
    directory: &'a Path,
    environment: Option<&'a str>,
    path: PathBuf,
}

impl<'a> Globals<'a> {
    pub fn new(storage: &'a Storage, directory: &'a Path, environment: Option<&'a str>) -> Self {
        Self {
            storage,
            directory,
            environment,
            path: storage.globals_path(directory, environment),
        }
    }

    pub fn load(&self) -> Result<Map<String, Value>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
            Err(error) => return Err(error).context("Cannot read saved response-script variables"),
        };
        if bytes.len() > MAX_GLOBAL_BYTES {
            bail!("Saved response-script variables exceed 10 MiB");
        }
        let saved: SavedGlobals = serde_json::from_slice(&bytes)
            .context("Cannot read saved response-script variables")?;
        if saved.directory != self.directory || saved.environment.as_deref() != self.environment {
            bail!("Saved response-script variables belong to a different project or environment");
        }
        Ok(saved.values)
    }

    pub fn commit(&self, changes: &[Mutation]) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Lock only the merge and atomic write, never the request or JavaScript execution.
        let lock = options.open(self.path.with_extension("lock"))?;
        lock.lock()?;
        let mut values = self.load()?;
        for change in changes {
            match change {
                Mutation::Set { name, value } => {
                    values.insert(name.clone(), value.clone());
                }
                Mutation::Clear { name } => {
                    values.remove(name);
                }
                Mutation::ClearAll => values.clear(),
            }
        }
        let saved = SavedGlobals {
            directory: self.directory.to_path_buf(),
            environment: self.environment.map(str::to_string),
            values,
        };
        let bytes = serde_json::to_vec(&saved)?;
        if bytes.len() > MAX_GLOBAL_BYTES {
            bail!("Response-script variables exceed 10 MiB. Previous variables were preserved.");
        }
        self.storage.write(&self.path, &bytes)
    }
}

pub fn variables(values: &Map<String, Value>) -> HashMap<String, String> {
    values
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_projects_and_environments_and_merges_parallel_updates() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::open(temp.path().join("cache")).unwrap();
        let directory = temp.path().join("project");
        std::thread::scope(|scope| {
            for name in ["first", "second"] {
                let storage = &storage;
                let directory = &directory;
                scope.spawn(move || {
                    Globals::new(storage, directory, Some("stage"))
                        .commit(&[Mutation::Set {
                            name: name.into(),
                            value: serde_json::json!([1, 2]),
                        }])
                        .unwrap();
                });
            }
        });
        let globals = Globals::new(&storage, &directory, Some("stage"));
        assert_eq!(globals.load().unwrap().len(), 2);
        assert!(
            Globals::new(&storage, &directory, Some("prod"))
                .load()
                .unwrap()
                .is_empty()
        );
        assert!(
            Globals::new(&storage, &temp.path().join("other"), Some("stage"))
                .load()
                .unwrap()
                .is_empty()
        );
        globals
            .commit(&[Mutation::Clear {
                name: "first".into(),
            }])
            .unwrap();
        assert_eq!(globals.load().unwrap().len(), 1);
        globals.commit(&[Mutation::ClearAll]).unwrap();
        assert!(globals.load().unwrap().is_empty());
    }
}
