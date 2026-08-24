//! Test-only helpers for env-var manipulation.
//!
//! Edition 2024 made `std::env::set_var` and `remove_var` `unsafe` because
//! they can race with reads from other threads. Every caller of these
//! helpers must be `#[serial]` (via `serial_test`), which serializes the
//! tests that touch env state and makes the mutation race-free in practice.

#![cfg(test)]

pub fn set_env(key: &str, val: &str) {
    // SAFETY: callers are `#[serial]`; no concurrent env access can race
    // with this mutation.
    unsafe { std::env::set_var(key, val) };
}

pub fn remove_env(key: &str) {
    // SAFETY: callers are `#[serial]`.
    unsafe { std::env::remove_var(key) };
}

pub fn remove_envs(keys: &[&str]) {
    for key in keys {
        remove_env(key);
    }
}

// ── Store fixture ─────────────────────────────────────────────────────────

use std::sync::Arc;

use anyhow::{Result, bail};

use crate::config::StoreConfig;
use crate::store::{SettingSpec, StoreProvider, VectorStore};

/// A backend that exists only to exercise the settings mechanism.
///
/// Deliberately *not* named after a real backend: core resolves whatever a
/// provider declares and must never know turbopuffer or nidus by name. The
/// real specs are covered in the crates that own those backends.
pub struct FixtureProvider;

fn fixture_default_path() -> String {
    "/default/fixture/path".to_string()
}

fn empty() -> String {
    String::new()
}

pub const FIXTURE_SETTINGS: &[SettingSpec] = &[
    SettingSpec {
        key: "path",
        env: "WDPKR_FIXTURE_PATH",
        secret: false,
        default: fixture_default_path,
        file_aliases: &[],
    },
    SettingSpec {
        key: "api_key",
        env: "WDPKR_FIXTURE_API_KEY",
        secret: true,
        default: empty,
        file_aliases: &["fixture_api_key"],
    },
];

impl StoreProvider for FixtureProvider {
    fn name(&self) -> &str {
        "fixture"
    }

    fn settings(&self) -> &'static [SettingSpec] {
        FIXTURE_SETTINGS
    }

    fn validate(&self, config: &StoreConfig) -> Result<()> {
        if config.get("fixture.path").is_empty() {
            bail!("store.fixture.path is required when store.provider=fixture");
        }
        Ok(())
    }

    fn build(&self, _config: &StoreConfig, _dimension: usize) -> Result<Box<dyn VectorStore>> {
        bail!("FixtureProvider builds no store")
    }
}

/// Register the fixture backend. Idempotent, so every test that needs backend
/// settings resolved can call it.
pub fn register_fixture() {
    crate::store::register_provider(Arc::new(FixtureProvider));
}

/// Env vars the fixture backend reads — clear these alongside the others.
pub const FIXTURE_ENVS: &[&str] = &["WDPKR_FIXTURE_PATH", "WDPKR_FIXTURE_API_KEY"];
