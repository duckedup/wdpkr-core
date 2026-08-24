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
# The logic lives in scripts/deps-check.sh, shared with the CI job so the gate
# and this recipe cannot drift — a stray `cargo add` is the one change that
# silently undoes this crate's reason to exist.
deps-check:
    ./scripts/deps-check.sh

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
    #!/usr/bin/env bash
    set -euo pipefail
    bd init --reinit-local --prefix wdpkr-core
    git config beads.role contributor
    chmod 700 .beads
    # Point the Dolt remote at this repo's git remote. Without this step
    # `bd dolt push` — step 4 of the session protocol in CLAUDE.md — silently
    # skips ("No remote is configured"), and the issue database never leaves the
    # machine. The transport is a custom git ref, refs/dolt/data on origin, which
    # is why the URL is the git remote rewritten as git+ssh://.
    url=$(git remote get-url origin | sed -E 's#^git@([^:]+):#git+ssh://git@\1/#')
    bd dolt remote add origin "$url"
    bd dolt push

# Push the beads issue database to origin's refs/dolt/data (session protocol
# step 4). Separate from `git push`: the issues live in Dolt, not in the commit.
bd-push:
    bd dolt push

# Land the current branch: gate locally, push, open a PR, queue it, sync beads.
# `main` is protected — the merge queue is the only way in, and it rebuilds the
# PR against current main before merging, so nothing lands red. Run from a
# feature branch (named for its issue); refuses on main.
pr *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    branch=$(git branch --show-current)
    if [ "$branch" = "main" ]; then
        echo "error: on main — branch first (git checkout -b <issue-id>)." >&2
        exit 1
    fi
    just ci
    git push -u origin "$branch"
    gh pr create --fill {{ ARGS }}
    # No --squash: the queue owns the merge method (SQUASH, set in the ruleset),
    # and passing one here just warns that it is being overridden.
    gh pr merge --auto
    bd dolt push
    echo "queued — it merges itself once the required checks are green."

# Remove all build artifacts
clean:
    cargo clean
