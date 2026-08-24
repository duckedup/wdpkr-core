//! Vector store abstraction — the seam this crate is built around.
//!
//! Defines the [`VectorStore`] trait for runtime operations, the
//! [`StoreProvider`] trait for backend registration, and a registry that
//! resolves a config string to a concrete implementation.
//!
//! **This module ships no backends.** wdpkr-core defines these traits and
//! never implements them: that is what keeps the crate backend-agnostic and
//! what lets nidus import it to implement [`VectorStore`] against itself
//! without a dependency cycle. `just deps-check` enforces it.
//!
//! Adding a backend, from a *consuming* crate:
//! 1. Implement [`VectorStore`] + [`StoreProvider`]
//! 2. Declare the backend's settings via [`StoreProvider::settings`] so the
//!    config layer can resolve them without knowing what the backend is
//! 3. Call [`register_provider`] before resolving config

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::StoreConfig;

// ── SettingSpec ───────────────────────────────────────────────────────────

/// Declares one backend setting so the config layer can resolve it through
/// the standard defaults → file → env chain **without naming the backend**.
///
/// A provider returns these from [`StoreProvider::settings`]; core reads the
/// list, resolves each key, and records per-field [`Source`] attribution. This
/// is how `wdpkr config list` can print `store.nidus.path` while core has
/// never heard of nidus.
///
/// [`Source`]: crate::config::Source
pub struct SettingSpec {
    /// Key relative to the backend's config section, e.g. `"path"` becomes
    /// `store.nidus.path` for a provider named `nidus`.
    pub key: &'static str,
    /// Env var that overrides the file value, e.g. `"WDPKR_NIDUS_PATH"`.
    pub env: &'static str,
    /// Secrets are resolved like any other setting but withheld from
    /// `config list` output.
    pub secret: bool,
    /// Hardcoded fallback, as a function so it can be computed (a path under
    /// `$XDG_DATA_HOME`, say) rather than a literal.
    pub default: fn() -> String,
    /// Deprecated flat file keys still honored for backwards compatibility,
    /// relative to `store.` — e.g. `"turbopuffer_api_key"`. Read, never
    /// written; the nested form wins when both are present.
    pub file_aliases: &'static [&'static str],
}

// ── StoreProvider ────────────────────────────────────────────────────────

/// Everything knowable about a backend *before* an instance of it exists:
/// its name, the settings it needs, whether those settings are valid, and how
/// to turn them into a live [`VectorStore`].
///
/// `validate` is deliberately separate from `build` so `wdpkr config` can
/// report a missing credential, and the indexer can fail fast, without
/// opening a connection.
pub trait StoreProvider: Send + Sync {
    /// Lowercase identifier matched against `store.provider`.
    fn name(&self) -> &str;

    /// Settings this backend reads. Defaults to none for backends that need
    /// no configuration.
    fn settings(&self) -> &'static [SettingSpec] {
        &[]
    }

    fn validate(&self, config: &StoreConfig) -> Result<()>;

    fn build(&self, config: &StoreConfig, dimension: usize) -> Result<Box<dyn VectorStore>>;
}

// ── Provider registry ────────────────────────────────────────────────────

static REGISTRY: LazyLock<RwLock<Vec<Arc<dyn StoreProvider>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Register a backend. Idempotent by name — re-registering replaces the
/// previous entry, so a consumer can override a default backend and tests can
/// re-register freely.
///
/// **Register before resolving config.** [`StoreConfig`] resolution reads
/// every registered provider's [`StoreProvider::settings`] to know which env
/// vars and defaults apply; a backend registered afterwards has no settings
/// resolved for it.
pub fn register_provider(provider: Arc<dyn StoreProvider>) {
    let mut reg = REGISTRY.write().expect("store registry poisoned");
    let name = provider.name().to_lowercase();
    reg.retain(|p| p.name().to_lowercase() != name);
    reg.push(provider);
}

/// Every registered backend, in registration order.
pub fn registered_providers() -> Vec<Arc<dyn StoreProvider>> {
    REGISTRY
        .read()
        .expect("store registry poisoned")
        .iter()
        .cloned()
        .collect()
}

/// Resolve a provider name (case-insensitive) to its registered backend.
pub fn resolve_provider(name: &str) -> Result<Arc<dyn StoreProvider>> {
    let needle = name.to_lowercase();
    let all = registered_providers();
    all.iter()
        .find(|p| p.name().to_lowercase() == needle)
        .cloned()
        .ok_or_else(|| {
            let available: Vec<String> = all.iter().map(|p| p.name().to_string()).collect();
            if available.is_empty() {
                anyhow!(
                    "unknown store provider '{name}': no store backends are registered. \
                     The consuming crate must call wdpkr_core::store::register_provider \
                     before resolving config."
                )
            } else {
                anyhow!(
                    "unknown store provider '{name}'. available: {}",
                    available.join(", ")
                )
            }
        })
}

/// Resolve `config.provider` and build the store it names.
pub fn build_store(config: &StoreConfig, dimension: usize) -> Result<Box<dyn VectorStore>> {
    let provider = resolve_provider(&config.provider)?;
    provider.build(config, dimension)
}

// ── VectorStore trait ────────────────────────────────────────────────────

#[async_trait]
pub trait VectorStore: Send + Sync {
    // ── Namespace lifecycle ──

    async fn create_namespace(&self, ns: &Namespace, dimension: usize) -> Result<()>;

    async fn delete_namespace(&self, ns: &Namespace) -> Result<()>;

    async fn namespace_exists(&self, ns: &Namespace) -> Result<bool>;

    // ── Metadata ──

    async fn get_metadata(&self, ns: &Namespace) -> Result<NamespaceMetadata>;

    async fn set_metadata(&self, ns: &Namespace, meta: &NamespaceMetadata) -> Result<()>;

    // ── Write ──

    /// Upsert a batch of documents. Idempotent — re-upserting the same
    /// ID with different content overwrites. Implementations should handle
    /// batching against provider limits internally.
    async fn upsert(&self, ns: &Namespace, docs: &[VectorDocument]) -> Result<UpsertStats>;

    /// Delete documents by ID.
    async fn delete_by_ids(&self, ns: &Namespace, ids: &[&str]) -> Result<()>;

    /// Delete all documents whose `file_path` matches the given path.
    /// More ergonomic than tracking IDs for file-level deletes during
    /// incremental indexing.
    async fn delete_by_file(&self, ns: &Namespace, file_path: &str) -> Result<()>;

    /// Delete all documents whose `file_path` matches a glob pattern.
    async fn delete_by_glob(&self, ns: &Namespace, pattern: &str) -> Result<usize>;

    /// Set `last_used_at` to `ts` (unix seconds) for every document whose
    /// `file_path` equals `file_path`, reusing the stored vectors (no
    /// re-embedding). Powers `wdpkr reinforce`. Returns the number of documents
    /// updated (0 if the path isn't indexed).
    async fn touch_by_file(&self, ns: &Namespace, file_path: &str, ts: i64) -> Result<usize>;

    /// Return a map of file_path → content_hash for all file-level documents
    /// in the namespace. Used by the indexer to skip unchanged files.
    async fn get_content_hashes(&self, ns: &Namespace) -> Result<HashMap<String, String>>;

    // ── Read ──

    /// Return all documents in the namespace, including vectors.
    /// Used by `--skip-summaries` to re-upsert with updated metadata.
    async fn list_documents(&self, ns: &Namespace) -> Result<Vec<VectorDocument>>;

    async fn search(
        &self,
        ns: &Namespace,
        query_vector: &[f32],
        opts: &SearchOptions,
    ) -> Result<Vec<SearchResult>>;
}

// ── Namespace ─────────────────────────────────────────────────────────────

/// Identifies a namespace. Derived from the git remote URL (normalized +
/// hashed) or overridden in config.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Namespace(pub String);

impl Namespace {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for Namespace {
    fn from(s: T) -> Self {
        Self(s.into())
    }
}

// ── Metadata ──────────────────────────────────────────────────────────────

/// Metadata stored alongside the namespace — not in the vectors themselves.
#[derive(Debug, Clone, Default)]
pub struct NamespaceMetadata {
    /// Last successfully indexed commit SHA.
    pub hwm_sha: Option<String>,
    /// Embedder provider + model used to create this namespace (e.g.
    /// "voyage/voyage-code-3"). Changing embedder requires a full reindex;
    /// the indexer checks this on startup and refuses incremental indexing
    /// if there's a mismatch.
    pub embedder: Option<String>,
    /// Arbitrary key-value pairs for future use.
    pub extra: HashMap<String, String>,
}

// ── ChunkKind ─────────────────────────────────────────────────────────────

/// Whether a vector represents a file-level summary or a symbol-level summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    File,
    Symbol,
}

impl std::fmt::Display for ChunkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkKind::File => write!(f, "file"),
            ChunkKind::Symbol => write!(f, "symbol"),
        }
    }
}

// ── VectorDocument ────────────────────────────────────────────────────────

/// A document to be stored in the vector store. Created by the indexer's
/// pipeline (chunk → summarize → embed → build VectorDocument → upsert).
#[derive(Debug, Clone)]
pub struct VectorDocument {
    /// Deterministic ID: hash of (file_path, chunk_kind, symbol_name,
    /// content_hash). Allows idempotent upserts.
    pub id: String,
    /// The dense embedding vector.
    pub vector: Vec<f32>,
    /// The summary text that was embedded (stored for debugging and
    /// returned in search results, not searched directly).
    pub summary: String,

    // ── Filterable attributes ──
    pub file_path: String,
    pub chunk_kind: ChunkKind,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub language: Option<String>,
    /// blake3 hash of the source file content. Used by the indexer to skip
    /// files whose content hasn't changed since last index.
    pub content_hash: Option<String>,
    /// Outbound call identifiers (unresolved). `None` = not yet indexed
    /// with call-graph data; `Some(vec![])` = genuinely no outbound calls.
    pub calls: Option<Vec<String>>,
    /// Symbols that reference this one. `None` = not yet indexed with
    /// call-graph data; `Some(vec![])` = genuinely no inbound callers.
    pub called_by: Option<Vec<String>>,
    /// Unix seconds when this document was last indexed or reinforced. Drives
    /// per-tap search-time decay (`wdpkr reinforce` bumps it). `None` for docs
    /// stored before decay existed — treated as "never decays".
    pub last_used_at: Option<i64>,
}

// ── SearchOptions ─────────────────────────────────────────────────────────

/// Query parameters for [`VectorStore::search`].
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub top_k: usize,
    pub path_prefixes: Vec<String>,
    pub chunk_kind: Option<ChunkKind>,
    pub language: Option<String>,
    pub min_score: Option<f32>,
}

// ── SearchResult ──────────────────────────────────────────────────────────

/// A single result from a vector search. Serializable to JSON for the
/// `wdpkr search` output.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub file_path: String,
    pub chunk_kind: ChunkKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub called_by: Option<Vec<String>>,
    /// Unix seconds when this document was last indexed or reinforced. Used by
    /// the search layer to apply per-tap decay; not part of the user JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
}

// ── UpsertStats ───────────────────────────────────────────────────────────

/// Counters returned by [`VectorStore::upsert`].
#[derive(Debug, Clone, Default)]
pub struct UpsertStats {
    pub upserted: usize,
    pub skipped: usize,
}
#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time assertions: these tests pass if the code compiles.
    // They verify trait bounds and object safety without needing a real
    // VectorStore implementation.

    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}

    #[test]
    fn types_are_send_and_sync() {
        _assert_send::<Namespace>();
        _assert_sync::<Namespace>();
        _assert_send::<NamespaceMetadata>();
        _assert_sync::<NamespaceMetadata>();
        _assert_send::<VectorDocument>();
        _assert_sync::<VectorDocument>();
        _assert_send::<SearchOptions>();
        _assert_sync::<SearchOptions>();
        _assert_send::<SearchResult>();
        _assert_sync::<SearchResult>();
        _assert_send::<UpsertStats>();
        _assert_sync::<UpsertStats>();
    }

    #[test]
    fn trait_is_object_safe() {
        // If this compiles, VectorStore can be used as `Box<dyn VectorStore>`.
        fn _takes_store(_: &dyn VectorStore) {}
    }

    #[test]
    fn namespace_from_string() {
        let ns = Namespace::from("my-repo");
        assert_eq!(ns.as_str(), "my-repo");
    }

    #[test]
    fn namespace_from_owned_string() {
        let ns = Namespace::from(String::from("my-repo"));
        assert_eq!(ns.as_str(), "my-repo");
    }

    #[test]
    fn namespace_equality() {
        let a = Namespace::from("repo-a");
        let b = Namespace::from("repo-a");
        let c = Namespace::from("repo-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn chunk_kind_display() {
        assert_eq!(ChunkKind::File.to_string(), "file");
        assert_eq!(ChunkKind::Symbol.to_string(), "symbol");
    }

    #[test]
    fn chunk_kind_json_round_trip() {
        let file_json = serde_json::to_string(&ChunkKind::File).unwrap();
        assert_eq!(file_json, r#""file""#);
        let back: ChunkKind = serde_json::from_str(&file_json).unwrap();
        assert_eq!(back, ChunkKind::File);

        let symbol_json = serde_json::to_string(&ChunkKind::Symbol).unwrap();
        assert_eq!(symbol_json, r#""symbol""#);
    }

    #[test]
    fn search_result_json_omits_none_fields() {
        let result = SearchResult {
            id: "abc".into(),
            score: 0.87,
            file_path: "src/main.rs".into(),
            chunk_kind: ChunkKind::File,
            symbol_name: None,
            symbol_kind: None,
            summary: "Entry point".into(),
            start_line: None,
            end_line: None,
            language: Some("rust".into()),
            calls: None,
            called_by: None,
            last_used_at: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("symbol_name"));
        assert!(!json.contains("symbol_kind"));
        assert!(!json.contains("start_line"));
        assert!(!json.contains("end_line"));
        assert!(json.contains("language"));
    }

    #[test]
    fn search_result_json_includes_symbol_fields() {
        let result = SearchResult {
            id: "def".into(),
            score: 0.91,
            file_path: "src/lib.rs".into(),
            chunk_kind: ChunkKind::Symbol,
            symbol_name: Some("run".into()),
            symbol_kind: Some("function".into()),
            summary: "Runs the app".into(),
            start_line: Some(10),
            end_line: Some(25),
            language: Some("rust".into()),
            calls: None,
            called_by: None,
            last_used_at: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""symbol_name":"run""#));
        assert!(json.contains(r#""symbol_kind":"function""#));
        assert!(json.contains(r#""start_line":10"#));
    }

    #[test]
    fn search_options_default() {
        let opts = SearchOptions::default();
        assert_eq!(opts.top_k, 0);
        assert!(opts.path_prefixes.is_empty());
        assert!(opts.chunk_kind.is_none());
        assert!(opts.language.is_none());
        assert!(opts.min_score.is_none());
    }

    #[test]
    fn namespace_metadata_default() {
        let meta = NamespaceMetadata::default();
        assert!(meta.hwm_sha.is_none());
        assert!(meta.embedder.is_none());
        assert!(meta.extra.is_empty());
    }

    #[test]
    fn upsert_stats_default() {
        let stats = UpsertStats::default();
        assert_eq!(stats.upserted, 0);
        assert_eq!(stats.skipped, 0);
    }

    #[test]
    fn namespace_usable_as_hash_key() {
        let mut map = HashMap::new();
        map.insert(Namespace::from("repo-a"), 1);
        map.insert(Namespace::from("repo-b"), 2);
        assert_eq!(map.get(&Namespace::from("repo-a")), Some(&1));
        assert_eq!(map.get(&Namespace::from("repo-b")), Some(&2));
        assert_eq!(map.get(&Namespace::from("repo-c")), None);
    }

    // ── Provider registry ────────────────────────────────────────────
    //
    // The registry is process-global, so these tests register uniquely-named
    // stub providers rather than sharing one fixture — that keeps them
    // order-independent without a mutex.

    struct StubProvider(&'static str);

    impl StoreProvider for StubProvider {
        fn name(&self) -> &str {
            self.0
        }

        fn validate(&self, _config: &StoreConfig) -> Result<()> {
            Ok(())
        }

        fn build(&self, _config: &StoreConfig, _dimension: usize) -> Result<Box<dyn VectorStore>> {
            Err(anyhow!("stub provider '{}' builds nothing", self.0))
        }
    }

    #[test]
    fn resolve_provider_finds_registered() {
        register_provider(Arc::new(StubProvider("stub-found")));
        let p = resolve_provider("stub-found").unwrap();
        assert_eq!(p.name(), "stub-found");
    }

    #[test]
    fn resolve_provider_is_case_insensitive() {
        register_provider(Arc::new(StubProvider("stub-case")));
        assert_eq!(resolve_provider("STUB-CASE").unwrap().name(), "stub-case");
        assert_eq!(resolve_provider("Stub-Case").unwrap().name(), "stub-case");
    }

    #[test]
    fn register_provider_replaces_same_name() {
        register_provider(Arc::new(StubProvider("stub-dup")));
        register_provider(Arc::new(StubProvider("stub-dup")));
        let count = registered_providers()
            .iter()
            .filter(|p| p.name() == "stub-dup")
            .count();
        assert_eq!(count, 1, "re-registering a name must not duplicate it");
    }

    #[test]
    fn resolve_provider_unknown_errors() {
        // Register something so we exercise the "available: ..." branch rather
        // than the empty-registry message.
        register_provider(Arc::new(StubProvider("stub-known")));
        // `.map(|_| ())` because Arc<dyn StoreProvider> has no Debug, which
        // `unwrap_err` requires of the Ok type.
        let err = resolve_provider("no-such-backend")
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown store provider"), "{err}");
        assert!(err.contains("stub-known"), "{err}");
    }

    #[test]
    fn settings_default_to_empty() {
        assert!(StubProvider("stub-settings").settings().is_empty());
    }
}
