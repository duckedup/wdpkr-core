# wdpkr-core

The backend-agnostic engine behind [wdpkr](https://github.com/duckedup/wdpkr): repo
walking, tree-sitter chunking, LLM summarization, embedding, and search
orchestration — everything except the vector store itself.

This is a library crate. There is no binary; the `wdpkr` CLI lives in the
[wdpkr](https://github.com/duckedup/wdpkr) repo.

## Why it exists

wdpkr indexes a codebase by summarizing it with an LLM and embedding the
summaries, then searches over those embeddings. That pipeline is useful
independently of *where* the vectors are kept — so it lives here, and storage
arrives through a trait:

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, ns: &Namespace, docs: &[VectorDocument]) -> Result<UpsertStats>;
    async fn search(&self, ns: &Namespace, query: &[f32], opts: &SearchOptions) -> Result<Vec<SearchResult>>;
    // …namespace lifecycle, metadata, deletes, content hashes
}
```

wdpkr-core defines that trait, the document and namespace types, and a provider
registry — and implements **no** backend. Which means:

| Repo | Role |
|---|---|
| **wdpkr-core** | The engine. Walk → chunk → summarize → embed → search. Knows nothing about storage. |
| **wdpkr** | The CLI. Imports this crate, registers the backends, glues the three together. |
| **nidus** | A backend. Imports this crate to implement `VectorStore` against itself. |

The invariant that makes this work: **wdpkr-core never depends on a concrete
store backend.** Without it, nidus implementing the trait would be a dependency
cycle. `just deps-check` enforces it in CI.

## Install

```toml
[dependencies]
wdpkr-core = "0.1"
```

## Layout

```
src/
├── config/       # 4-layer resolution: defaults → file → env → flags
├── chunk/        # tree-sitter AST chunking (8 languages)
├── ai_providers/ # embed + summarize model adapters and a capability registry
├── http/         # shared reqwest client with bounded retry
├── summarize/    # Summarizer trait, prompts, big-file rollup
├── embed/        # Embedder trait + factory
├── store/        # VectorStore + StoreProvider traits, types, registry — no backends
├── search/       # search orchestration + output
├── indexer/      # git diff → walk → chunk → summarize → embed → upsert
├── tap/          # source adapters: files, linear, notion, process
├── decision/     # store-native architectural decision records
├── eval/         # retrieval-quality harness
└── testing/      # mocks + fixtures
```

## Development

```bash
just             # list recipes
just ci          # fmt-check + clippy + test + deps-check
just deps-check  # assert no store backend crept into the dep tree
just miri        # UB check (requires nightly)
```

Rust 1.96+ (pinned via `rust-toolchain.toml`), edition 2024.

Changing the public API? It is a contract with two other repos — build them
against this checkout before publishing:

```toml
# in the consumer's Cargo.toml
[patch.crates-io]
wdpkr-core = { path = "../wdpkr-core" }
```

```bash
just check-consumer wdpkr
```

## License

MIT
