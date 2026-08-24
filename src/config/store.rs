//! Store configuration — **backend-opaque**.
//!
//! Core cannot name a backend, so `StoreConfig` holds `provider` plus a bag of
//! resolved per-backend settings keyed by `<provider>.<key>`. Each registered
//! [`StoreProvider`] declares what it needs via [`SettingSpec`]; core runs the
//! same defaults → file → env chain used everywhere else over those
//! declarations and records per-field [`Source`] attribution.
//!
//! The consequence is that `wdpkr config list` can print `store.nidus.path`,
//! sourced from `WDPKR_NIDUS_PATH`, while this crate has never heard of nidus.
//!
//! **Providers must be registered before resolution** — see
//! [`crate::store::register_provider`]. Settings are only resolved for
//! backends the registry knows about at the time `resolve` runs.
//!
//! [`StoreProvider`]: crate::store::StoreProvider
//! [`SettingSpec`]: crate::store::SettingSpec

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use super::{FileConfig, Resolved, Source, env_or_resolved, file_or_resolved};

/// Resolved store configuration: the chosen provider plus its settings.
///
/// Settings are addressed by their fully-qualified key, `<provider>.<key>` —
/// e.g. `config.get("turbopuffer.api_key")`. The bag holds settings for
/// *every* registered backend, not just the active one, so `config list` can
/// show them all.
#[derive(Clone, Default)]
pub struct StoreConfig {
    pub provider: String,
    settings: BTreeMap<String, String>,
}

/// Per-field source attribution paralleling [`StoreConfig`].
#[derive(Debug, Clone, Default)]
pub struct StoreSources {
    pub provider: Source,
    settings: BTreeMap<String, Source>,
}

impl StoreConfig {
    /// Build directly from a provider name and settings — for consumers and
    /// tests that bypass file/env resolution.
    pub fn new<K, V>(
        provider: impl Into<String>,
        settings: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            provider: provider.into(),
            settings: settings
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    /// Look up a setting by fully-qualified key, e.g. `"nidus.path"`.
    /// Returns `""` when unset — mirroring how backends already treat a
    /// missing credential.
    pub fn get(&self, key: &str) -> &str {
        self.settings
            .get(key)
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Like [`Self::get`], but `None` rather than `""` for missing or empty.
    pub fn get_opt(&self, key: &str) -> Option<&str> {
        self.settings
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// Every resolved setting key, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.settings.keys().map(String::as_str)
    }

    /// Check the active provider is registered and its settings are usable.
    /// Delegates the backend-specific part to the provider itself.
    pub fn validate(&self) -> Result<()> {
        if self.provider.trim().is_empty() {
            let available: Vec<String> = crate::store::registered_providers()
                .iter()
                .map(|p| p.name().to_string())
                .collect();
            if available.is_empty() {
                bail!(
                    "no store backend is registered. The consuming crate must call \
                     wdpkr_core::store::register_provider before resolving config."
                );
            }
            bail!(
                "store.provider is not set. available: {}",
                available.join(", ")
            );
        }
        let provider = crate::store::resolve_provider(&self.provider)?;
        provider.validate(self)
    }

    /// Convenience: drop the source map.
    pub fn from_env(file: &Option<FileConfig>) -> Self {
        Self::resolve(file).0
    }

    /// Canonical resolver — returns values + per-field sources.
    pub fn resolve(file: &Option<FileConfig>) -> (Self, StoreSources) {
        let f = file.as_ref().and_then(|f| f.store.as_ref());

        let provider: Resolved<String> = env_or_resolved(
            "WDPKR_STORE_PROVIDER",
            file_or_resolved(
                f.and_then(|s| s.provider.clone()).filter(|p| !p.is_empty()),
                // No default: core cannot prefer a backend it does not know.
                // An unset provider is caught by `validate`, which lists what
                // is registered. A consumer wanting a zero-config default
                // supplies it in its own config layer.
                String::new(),
            ),
        );

        let mut settings = BTreeMap::new();
        let mut sources = BTreeMap::new();

        for backend in crate::store::registered_providers() {
            for spec in backend.settings() {
                // Nested `store.<backend>.<key>` wins; deprecated flat
                // aliases under `store.` are read only as a fallback.
                let from_file = f
                    .and_then(|s| s.nested(backend.name(), spec.key))
                    .or_else(|| {
                        spec.file_aliases
                            .iter()
                            .find_map(|alias| f.and_then(|s| s.alias(alias)))
                    });

                let resolved: Resolved<String> =
                    env_or_resolved(spec.env, file_or_resolved(from_file, (spec.default)()));

                let key = format!("{}.{}", backend.name(), spec.key);
                settings.insert(key.clone(), resolved.value);
                sources.insert(key, resolved.source);
            }
        }

        (
            Self {
                provider: provider.value,
                settings,
            },
            StoreSources {
                provider: provider.source,
                settings: sources,
            },
        )
    }
}

impl StoreSources {
    /// Source for a fully-qualified setting key, if it was resolved.
    pub fn get(&self, key: &str) -> Option<&Source> {
        self.settings.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_helpers::{register_fixture, remove_envs, set_env};
    use crate::config::{FileConfig, FileStoreConfig};
    use serial_test::serial;

    fn clear_env() {
        remove_envs(&["WDPKR_STORE_PROVIDER"]);
        remove_envs(crate::config::test_helpers::FIXTURE_ENVS);
    }

    /// A `FileConfig` carrying only a `store:` block.
    fn file_with(store: FileStoreConfig) -> Option<FileConfig> {
        Some(FileConfig {
            store: Some(store),
            ..Default::default()
        })
    }

    fn nested_file(backend: &str, key: &str, value: &str) -> Option<FileConfig> {
        let mut s = FileStoreConfig::default();
        s.set_nested(backend, key, value);
        file_with(s)
    }

    // ── provider ──────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn provider_defaults_to_empty() {
        clear_env();
        let cfg = StoreConfig::from_env(&None);
        assert_eq!(cfg.provider, "", "core must not prefer a backend");
    }

    #[test]
    #[serial]
    fn provider_from_env_beats_file() {
        clear_env();
        set_env("WDPKR_STORE_PROVIDER", "from-env");
        let file = file_with(FileStoreConfig {
            provider: Some("from-file".into()),
            ..Default::default()
        });
        let cfg = StoreConfig::from_env(&file);
        assert_eq!(cfg.provider, "from-env");
        clear_env();
    }

    #[test]
    #[serial]
    fn provider_from_file_when_env_absent() {
        clear_env();
        let file = file_with(FileStoreConfig {
            provider: Some("from-file".into()),
            ..Default::default()
        });
        assert_eq!(StoreConfig::from_env(&file).provider, "from-file");
    }

    // ── backend settings: the four layers ─────────────────────────────

    #[test]
    #[serial]
    fn setting_falls_back_to_provider_declared_default() {
        clear_env();
        register_fixture();
        let cfg = StoreConfig::from_env(&None);
        assert_eq!(cfg.get("fixture.path"), "/default/fixture/path");
    }

    #[test]
    #[serial]
    fn setting_read_from_nested_file_block() {
        clear_env();
        register_fixture();
        let file = nested_file("fixture", "path", "/from/file");
        assert_eq!(
            StoreConfig::from_env(&file).get("fixture.path"),
            "/from/file"
        );
    }

    #[test]
    #[serial]
    fn setting_env_beats_file() {
        clear_env();
        register_fixture();
        set_env("WDPKR_FIXTURE_PATH", "/from/env");
        let file = nested_file("fixture", "path", "/from/file");
        assert_eq!(
            StoreConfig::from_env(&file).get("fixture.path"),
            "/from/env"
        );
        clear_env();
    }

    #[test]
    #[serial]
    fn deprecated_flat_alias_is_still_read() {
        clear_env();
        register_fixture();
        let mut s = FileStoreConfig::default();
        s.backends.insert(
            "fixture_api_key".into(),
            serde_yaml::Value::String("legacy".into()),
        );
        assert_eq!(
            StoreConfig::from_env(&file_with(s)).get("fixture.api_key"),
            "legacy"
        );
    }

    #[test]
    #[serial]
    fn nested_form_beats_deprecated_alias() {
        clear_env();
        register_fixture();
        let mut s = FileStoreConfig::default();
        s.set_nested("fixture", "api_key", "nested");
        s.backends.insert(
            "fixture_api_key".into(),
            serde_yaml::Value::String("legacy".into()),
        );
        assert_eq!(
            StoreConfig::from_env(&file_with(s)).get("fixture.api_key"),
            "nested"
        );
    }

    // ── accessors ─────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn unset_key_reads_as_empty_and_none() {
        clear_env();
        register_fixture();
        let cfg = StoreConfig::from_env(&None);
        assert_eq!(cfg.get("fixture.api_key"), "");
        assert_eq!(cfg.get_opt("fixture.api_key"), None);
        assert_eq!(cfg.get("no-such-backend.key"), "");
    }

    #[test]
    fn new_builds_directly() {
        let cfg = StoreConfig::new("direct", [("direct.path", "/p")]);
        assert_eq!(cfg.provider, "direct");
        assert_eq!(cfg.get("direct.path"), "/p");
        assert_eq!(cfg.keys().collect::<Vec<_>>(), vec!["direct.path"]);
    }

    // ── source attribution ────────────────────────────────────────────

    #[test]
    #[serial]
    fn sources_track_where_each_setting_came_from() {
        clear_env();
        register_fixture();
        set_env("WDPKR_FIXTURE_API_KEY", "k");
        let file = nested_file("fixture", "path", "/from/file");
        let (_, sources) = StoreConfig::resolve(&file);
        assert_eq!(sources.get("fixture.path"), Some(&Source::File));
        assert_eq!(
            sources.get("fixture.api_key"),
            Some(&Source::Env("WDPKR_FIXTURE_API_KEY"))
        );
        clear_env();

        let (_, sources) = StoreConfig::resolve(&None);
        assert_eq!(sources.get("fixture.path"), Some(&Source::Default));
        assert_eq!(sources.provider, Source::Default);
    }

    // ── validate ──────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn validate_rejects_unset_provider() {
        clear_env();
        register_fixture();
        let err = StoreConfig::from_env(&None)
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("store.provider is not set"), "{err}");
        assert!(
            err.contains("fixture"),
            "error should list what is available: {err}"
        );
    }

    #[test]
    #[serial]
    fn validate_rejects_unknown_provider() {
        clear_env();
        register_fixture();
        let err = StoreConfig::new("no-such-backend", [("x", "y")])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown store provider"), "{err}");
    }

    #[test]
    #[serial]
    fn validate_delegates_to_the_provider() {
        clear_env();
        register_fixture();
        // The fixture requires `path`; supplying none must fail its own check.
        let err = StoreConfig::new("fixture", [("fixture.path", "")])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("store.fixture.path is required"), "{err}");

        let (mut cfg, _) = StoreConfig::resolve(&None);
        cfg.provider = "fixture".into();
        assert!(cfg.validate().is_ok(), "the declared default satisfies it");
    }
}
