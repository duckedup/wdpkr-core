# wdpkr-core — task runner.
# Run `just` (or `just --list`) to see available recipes.

# Default: list recipes
default:
    @just --list

# ── Local dev ──────────────────────────────────────────────────────────────

# Quick compile check across all targets
check:
    cargo check --all-targets

# Format all code
fmt:
    cargo fmt --all

# Verify formatting is clean (CI guard)
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy, deny all warnings
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
test:
    cargo test --all-features

# Run tests within a single module/path (e.g. `just test-mod config`)
test-mod MOD:
    cargo test --all-features {{ MOD }}

# Generate and open the API docs
doc:
    cargo doc --no-deps --open

# Show the dependency tree. wdpkr-core must never gain a dependency on a
# concrete store backend (turbopuffer, nidus) — that is the whole point of the
# split, and this is how you check it.
deps:
    cargo tree

# Assert the backend-agnostic invariant: no store backend in the dep tree.
# Kept as a recipe (and a CI job) because a stray `cargo add` is the one change
# that silently undoes this crate's reason to exist.
deps-check:
    #!/usr/bin/env bash
    set -euo pipefail
    tree=$(cargo tree --prefix none --format '{p}' --all-features)
    for forbidden in nidus turbopuffer; do
        if echo "$tree" | grep -qE "^${forbidden} v"; then
            echo "error: wdpkr-core must not depend on '$forbidden' — it is a store backend." >&2
            exit 1
        fi
    done
    echo "ok: no store backend in the dependency tree"

# Run Miri to check for undefined behavior (requires nightly).
# wdpkr-core is pure Rust apart from tree-sitter's C parsers, so Miri builds the
# crate; tests needing OS-level FFI (tokio reactor, tree-sitter, subprocesses)
# carry #[cfg_attr(miri, ignore)].
miri:
    MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-permissive-provenance -Zmiri-ignore-leaks" cargo +nightly miri test

# Pre-commit / pre-PR checks: format clean, no clippy warnings, tests green,
# no store backend in the dep tree
ci: fmt-check lint test deps-check

# ── Build ──────────────────────────────────────────────────────────────────

# Debug build
build:
    cargo build

# Release build
release:
    cargo build --release

# Dry-run the crates.io publish (what the release workflow does on a version bump)
publish-check:
    cargo publish --dry-run

# ── Consumers ──────────────────────────────────────────────────────────────
# wdpkr and nidus both import this crate. When changing a public API, check the
# change against a local checkout before publishing, via a [patch.crates-io]
# entry pointing at this directory.

# Build a sibling consumer against this checkout (e.g. `just check-consumer wdpkr`)
check-consumer NAME:
    cd ../{{ NAME }} && cargo check --all-targets

# ── Project ────────────────────────────────────────────────────────────────

# Initialize beads issue tracking for this project
bd-init:
    bd init --reinit-local --prefix wdpkr-core
    git config beads.role contributor
    chmod 700 .beads

# Remove all build artifacts
clean:
    cargo clean
