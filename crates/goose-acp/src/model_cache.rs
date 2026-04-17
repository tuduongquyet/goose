//! Disk-persisted cache of per-provider model lists.
//!
//! When a user picks an ACP provider (e.g. Claude Code) the backend has to
//! spawn the external agent and wait for its initial `NewSession` before it
//! can answer "what models do you have?". That round-trip is ~25s for
//! claude-acp on cold start. This cache lets the UI fill the model picker
//! instantly with the last-known list while the real `update_provider` call
//! continues in the background.
//!
//! The cache stores the raw `Vec<SessionConfigOption>` so it can be replayed
//! verbatim through the existing `ConfigOptionUpdate` notification path.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs_err as fs;
use goose::config::paths::Paths;
use sacp::schema::SessionConfigOption;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CACHE_FILE_NAME: &str = "acp_model_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCache {
    #[serde(default)]
    pub providers: HashMap<String, ProviderEntry>,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub options: Vec<SessionConfigOption>,
    pub cached_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

impl ModelCache {
    pub fn default_path() -> PathBuf {
        Paths::in_state_dir(CACHE_FILE_NAME)
    }

    pub fn load() -> Self {
        Self::load_from(Self::default_path())
    }

    pub fn load_from(path: PathBuf) -> Self {
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<ModelCache>(&bytes) {
                Ok(mut cache) => {
                    cache.path = path;
                    cache
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "ACP model cache could not be parsed, starting empty",
                    );
                    Self {
                        providers: HashMap::new(),
                        path,
                    }
                }
            },
            Err(_) => Self {
                providers: HashMap::new(),
                path,
            },
        }
    }

    pub fn get(&self, provider_name: &str) -> Option<&ProviderEntry> {
        self.providers.get(provider_name)
    }

    /// Returns the provider name with the most recent `last_used_at`.
    pub fn last_used_provider(&self) -> Option<String> {
        self.providers
            .iter()
            .max_by_key(|(_, entry)| entry.last_used_at)
            .map(|(name, _)| name.clone())
    }

    pub fn upsert(&mut self, provider_name: &str, options: Vec<SessionConfigOption>) {
        let now = Utc::now();
        self.providers.insert(
            provider_name.to_string(),
            ProviderEntry {
                options,
                cached_at: now,
                last_used_at: now,
            },
        );
    }

    pub fn save(&self) -> Result<()> {
        Self::save_to(&self.path, self)
    }

    fn save_to(path: &Path, cache: &ModelCache) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("creating model cache parent dir {}", parent.display())
            })?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes =
            serde_json::to_vec_pretty(cache).context("serializing model cache to json")?;
        fs::write(&tmp, &bytes)
            .with_context(|| format!("writing temp model cache {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("renaming model cache to {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sacp::schema::SessionConfigOption;
    use tempfile::TempDir;

    fn sample_options() -> Vec<SessionConfigOption> {
        vec![SessionConfigOption::select(
            "model",
            "Model",
            "opus".to_string(),
            vec![],
        )]
    }

    #[test]
    fn round_trip_through_disk() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("acp_model_cache.json");

        let mut cache = ModelCache::load_from(path.clone());
        assert!(cache.providers.is_empty());

        cache.upsert("claude-acp", sample_options());
        cache.save().unwrap();

        let reloaded = ModelCache::load_from(path);
        let entry = reloaded.get("claude-acp").expect("entry exists");
        assert_eq!(entry.options.len(), 1);
    }

    #[test]
    fn missing_file_yields_empty_cache() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does_not_exist.json");
        let cache = ModelCache::load_from(path);
        assert!(cache.providers.is_empty());
    }

    #[test]
    fn last_used_provider_picks_most_recent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("acp_model_cache.json");
        let mut cache = ModelCache::load_from(path);
        cache.upsert("claude-acp", sample_options());
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.upsert("codex", sample_options());
        assert_eq!(cache.last_used_provider().as_deref(), Some("codex"));
    }

    #[test]
    fn corrupt_file_yields_empty_cache() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("acp_model_cache.json");
        fs::write(&path, b"not json").unwrap();
        let cache = ModelCache::load_from(path);
        assert!(cache.providers.is_empty());
    }
}
