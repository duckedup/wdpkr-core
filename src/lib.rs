//! wdpkr-core — the backend-agnostic engine behind [wdpkr].
//!
//! This crate owns the logic that makes wdpkr wdpkr: walking a repo, chunking
//! it with tree-sitter, summarizing chunks with an LLM, embedding the
//! summaries, and searching over them. What it deliberately does *not* own is
//! any concrete vector-store backend.
//!
//! Storage arrives through the `VectorStore` trait: this crate defines the
//! trait, the document/namespace types, and the provider registry, and knows
//! nothing about Turbopuffer or nidus. The `wdpkr` binary registers those
//! backends and glues the pieces together; nidus can implement the same trait
//! against itself without depending on either.
//!
//! [wdpkr]: https://github.com/duckedup/wdpkr

// Modules land here as they are extracted from the wdpkr repo — see `bd ready`.
