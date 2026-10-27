//! Load the nearest public/private environment pair with explicit precedence.
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Environments {
    pub directory: Option<PathBuf>,
    public: Map<String, Value>,
    private: Map<String, Value>,
}

impl Environments {
    pub fn load(request_file: &Path) -> Result<Self> {
        // The nearest directory containing either file defines the environment set.
        for dir in request_file.parent().into_iter().flat_map(Path::ancestors) {
            let public = dir.join("http-client.env.json");
            let private = dir.join("http-client.private.env.json");
            if public.exists() || private.exists() {
                return Ok(Self {
                    directory: Some(dir.to_path_buf()),
                    public: read(&public)?,
                    private: read(&private)?,
                });
            }
        }
        Ok(Self::default())
    }

    pub fn names(&self) -> Vec<String> {
        self.public
            .keys()
            .chain(self.private.keys())
            .filter(|name| *name != "$shared")
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn variables(&self, name: Option<&str>) -> Result<HashMap<String, String>> {
        let mut variables = HashMap::new();
        if let Some(name) = name
            && !self.public.contains_key(name)
            && !self.private.contains_key(name)
        {
            bail!("Unknown HTTP environment: {name}");
        }
        for source in [&self.public, &self.private] {
            for section in [Some("$shared"), name].into_iter().flatten() {
                if let Some(values) = source.get(section) {
                    let values = values
                        .as_object()
                        .context("Environment must be a JSON object")?;
                    for (key, value) in values {
                        variables.insert(
                            key.clone(),
                            value
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| value.to_string()),
                        );
                    }
                }
            }
        }
        Ok(variables)
    }
}

fn read(path: &Path) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|_| anyhow::anyhow!("Invalid environment JSON in {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(_) => bail!("Cannot read environment file {}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_values_override_public_values() {
        let environments = Environments {
            directory: None,
            public: serde_json::from_str(
                r#"{"$shared":{"x":"shared"},"local":{"x":"public","port":8000}}"#,
            )
            .unwrap(),
            private: serde_json::from_str(r#"{"local":{"x":"private"}}"#).unwrap(),
        };
        assert_eq!(environments.names(), vec!["local"]);
        let variables = environments.variables(Some("local")).unwrap();
        assert_eq!(variables["x"], "private");
        assert_eq!(variables["port"], "8000");
        assert!(environments.variables(Some("missing")).is_err());
    }
}
