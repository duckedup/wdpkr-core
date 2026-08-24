# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

## Landing code: branch → PR → merge queue

**`main` is protected. Nothing is pushed to it directly** — this overrides step 4
of the session protocol above, whose bare `git push` no longer applies to code.
The ruleset rejects direct pushes and force-pushes, so a session that tries one
fails at the push.

```bash
git checkout -b <issue-id>          # one branch per issue or bundled epic
just ci                             # green locally before you ask CI
git push -u origin <issue-id>
gh pr create --fill                 # PR is the only way in
gh pr merge --auto --squash         # queue it; it merges itself when green
bd dolt push                        # step 4's other half still applies
```

`--auto` hands the PR to the **merge queue**: GitHub rebuilds it against the
current `main` (which catches the PR that is green alone but broken combined),
and merges it by squash once every required check passes. Nobody babysits it, and
nothing merges red.

Required checks — all five of these, on the PR *and* on the queue's temporary
ref: `fmt`, `clippy`, `test`, `release`, `No store backend`.

`Miri` is **not** required. It still runs on every PR, but it is advisory: it
interprets MIR, so its runtime tracks the size of the test suite, and it skips
`merge_group` so a queue entry never waits on it. Read it, don't be gated by it.

**Adding a required check and adding its `merge_group:` trigger are one change,
never two.** A queued PR builds on a `gh-readonly-queue/**` ref; a workflow that
owns a required check but does not list `merge_group:` under `on:` never reports
there, and the entry stalls until it is ejected. The queue looks broken when it is
really just waiting. For the same reason, narrow a *required* job's work with a
per-step `if:`, never a job-level one — a job skipped outright is exactly the
check that never reports.

The rule runs in reverse when retiring a check: drop it from the ruleset's
required list **first**, then stop it running on `merge_group`. Do it the other
way round and every entry stalls on a check that can no longer report. Only a
non-required job may carry a job-level `if:` — which is exactly how `Miri`
skips the queue.

Review is a human's call, not a gate: `required_approving_review_count` is 0, so
the queue is what enforces correctness. Ask for review when the change deserves
it; don't wait on it to land routine work.

### Issue state never lands on main

The issue database is a Dolt database under `.beads/` — **local and gitignored**.
It is shared by pushing to `refs/dolt/data` on `origin`, this repo itself, not a
separate service. So **`bd dolt push` is as load-bearing as `git push`**: without
it your issue changes exist on your machine only, and no PR carries them. `just
bd-sync` (pull then push) is the tracker half of finishing a session; `just pr`
already runs it.

What *is* tracked in git is only the handful of config files that let a fresh
clone find that database: `.beads/config.yaml`, `.beads/metadata.json`,
`.beads/.gitignore`, `.beads/hooks/`. Everything else under `.beads/` is runtime.

**Never let the JSONL export become tracked.** `.beads/issues.jsonl` is a local
viewer/backup artifact — gitignored, with `export.git-add: false`. A tracked
export is rewritten from whichever branch's local database happens to commit it,
so any branch can silently revert another's closes; nidus hit exactly that. With
a merge queue rebasing entries against a moving `main`, a tracked export is worse
than useless. The Dolt ref is the only shared state.

**In a fresh clone, run `just bd-setup`** — it reads the tracked config, recovers
the database from `refs/dolt/data`, and wires the remote. Needs the `dolt` CLI
(`brew install dolt`); `bd` alone can push but cannot clone. Safe to re-run: an
existing database is left untouched, because it may hold unpushed work.

It is deliberately **not** `bd bootstrap`, and never `bd init`. Bootstrap cannot
reach this repo's own `refs/dolt/data` and leaves a fresh clone with an *empty*
tracker and no error naming the cause; the "recovery" it then offers would
force-push nothing over everyone's issues. `bd init` mints a new identity and can
do the same. **If you ever see a `bd dolt push --force` prompt, stop.**

**Close the ticket yourself when the PR merges — nothing auto-closes.** A
`Closes wdpkr-core-abc` line in a PR body is documentation; GitHub cannot close a
bead. Run `bd close <id>` and `just bd-sync` as part of shipping, and confirm the
merged diff actually finishes the issue before you do.

## What this repo is

wdpkr-core is the **backend-agnostic engine** behind [wdpkr](https://github.com/duckedup/wdpkr): the
walking, the chunking, the summarizing, the embedding, the search. It is a
library crate — there is no binary here.

Three repos, one split:

| Repo | Owns |
|---|---|
| **wdpkr-core** (this) | Repo walk, tree-sitter chunking, summarization, embedding, search orchestration, config, taps, decisions, the `VectorStore` trait + provider registry |
| **wdpkr** | The `wdpkr` CLI. Glues the three together: imports this crate and registers the concrete store backends |
| **nidus** | A store backend. Imports this crate to implement `VectorStore` against itself |

**The invariant: wdpkr-core must never depend on a concrete store backend.**
Not Turbopuffer, not nidus. Storage arrives only through the `VectorStore`
trait, which this crate defines and neither implements. Breaking that makes the
split pointless and creates a dependency cycle with nidus. `just deps-check`
enforces it (in `just ci` and as its own CI job) — if you find yourself wanting
to reach for a backend here, the code you are writing belongs in wdpkr instead.

## Build & Test

```bash
just test          # run all tests
just ci            # fmt-check + clippy (-D warnings) + test + deps-check
just lint          # clippy only
just deps-check    # assert no store backend in the dependency tree
just deps          # print the dependency tree
just miri          # undefined behavior check via Miri (requires nightly)
just fmt           # format code
just doc           # build and open the API docs
just build         # debug build
```

Rust 1.96+ required (pinned via `rust-toolchain.toml`). Edition 2024.

### Every Rust change bumps the version

**If a PR touches `.rs`, it bumps `version` in `Cargo.toml`.** Not just breaking
changes — every one. Two repos consume this crate from crates.io, and a change
they cannot name is a change they cannot adopt: `cargo update` has nothing to
move to, and "which version has that fix" has no answer.

Docs-only, CI-only, or justfile-only PRs do not need a bump.

Pick the bump the change deserves, pre-1.0:

| Change | Bump |
|---|---|
| Anything a consumer must edit code for — a removed or re-shaped public item | minor (`0.2` → `0.3`) |
| New public API, or internal-only changes and fixes | patch (`0.2.0` → `0.2.1`) |

Landing the bump on `main` is the whole release process: `release.yml` sees the
new version, publishes to crates.io, tags `v<version>`, and cuts a GitHub
release. There is no separate publish step, and no way to land a Rust change
*without* releasing it — which is the point.

Before merging a bump, run `just publish-check` (a `cargo publish --dry-run`);
it needs a clean tree, so commit first. To hold releases back, set
`publish = false` in `Cargo.toml` — a one-line kill switch the workflow checks.

### Checking consumers

A public API change here breaks wdpkr and nidus, and neither breakage shows up
in this crate's CI. Before publishing, point the consumer at this checkout and
build it:

```toml
# in the consumer's Cargo.toml
[patch.crates-io]
wdpkr-core = { path = "../wdpkr-core" }
```

```bash
just check-consumer wdpkr    # then the same for nidus
```

### Miri (Undefined Behavior Checker)

`just miri` runs the test suite under [Miri](https://github.com/rust-lang/miri/)
to detect undefined behavior, memory leaks, and pointer provenance issues. Miri
interprets MIR (Rust's mid-level IR) and cannot execute FFI calls.

**When to add `#[cfg_attr(miri, ignore)]`** to a test:
- Uses `#[tokio::test]` — tokio's runtime requires kqueue/epoll (OS-level FFI)
- Calls tree-sitter (`Parser::new()`, `TreeSitterChunker`) — C library FFI
- Spawns processes (`Command::new()`) — requires `fork()` syscall
- Creates reqwest `Client` directly (not via mocks) — system TLS FFI

**Do NOT ignore** tests that use mock implementations (`MockEmbedder`,
`MockVectorStore`, `MockSummarizer`) — these run under Miri.

Miri runs in CI as its own job on pull requests, and is **not** a required
check: a red Miri never blocks a merge, whether the cause is real UB or just a
broken nightly. Treat a failure as a bug to chase, not a gate to satisfy — and
since nothing enforces it, actually read it. It does not run on the merge
queue.

## Architecture Overview

```
src/
├── config/       # 4-layer resolution: defaults → file → env → CLI flags
├── chunk/        # tree-sitter AST chunking (8 languages)
├── ai_providers/ # All model-backend adapters: voyage/openai/ollama (embed) + anthropic (summarize) + capability registry
├── http/         # Shared reqwest retry: RetryPolicy + send_with_retry
├── summarize/    # Summarizer trait + prompt templates + big-file rollup + build_summarizer factory
├── embed/        # Embedder trait + build_embedder factory
├── store/        # VectorStore + StoreProvider traits, document/namespace types, provider registry — NO backends
├── search/       # Search orchestration + JSON/pretty output
├── indexer/      # Full pipeline: git diff → walk → chunk → summarize → embed → upsert
├── tap/          # Source adapters: files, linear, notion, process
├── decision/     # Store-native architectural decision records
├── eval/         # Retrieval-quality harness + metrics
└── testing/      # Mocks (store, embedder, summarizer, tap) + fixtures
```

Provider adapters live in one place (`ai_providers/`); the `embed` and
`summarize` modules own their traits and a factory that consults
`ai_providers::PROVIDERS` (a capability registry — `Embed`/`Summarize`) before
dispatching. Voyage is embed-only by design. All HTTP adapters share
`http::send_with_retry`: a reqwest client, bounded exponential-backoff retry on
transient send errors and retryable statuses, configurable base URL for testing.

`store/` is the seam. It defines `VectorStore` (runtime ops), `StoreProvider`
(backend registration), and a registry that resolves a config string to a
concrete implementation — but ships **no** implementations beyond the mock in
`testing/`. Consumers register their own: adding a backend means adding a
`StoreProvider` in *their* crate, not a module here.

## Conventions & Patterns

- **No store backends**: see the invariant above. `just deps-check` is the gate.
- **Trait-first design**: VectorStore, Embedder, Summarizer, Chunker are all traits with mock + real implementations
- **Config via `env_or` pattern**: `env_or_resolved(KEY, file_or_resolved(file_value, default))` — every field has a known env var, file key, and hardcoded default
- **Tests are mock-based**: no live API calls in the test suite. Integration tests create temp git repos with fixture source files
- **Public API is a contract**: two repos depend on it. A breaking change needs a `just check-consumer` pass against both — see *Every Rust change bumps the version* below
- **Commit style**: emoji prefix + short description (e.g. `🔍 search orchestration`)
- **Issue tracking**: `bd` (beads) — run `bd ready` for available work
- **Branch workflow**: one branch per issue or bundled epic, push for PR review
- **Error handling**: `anyhow` at the trait boundary, traits return `anyhow::Result`
- **Async runtime**: `tokio` — the crate stays runtime-agnostic where it can; consumers own the runtime

### wdpkr

This repo has a semantic codebase index via `wdpkr`. Use it to **locate feature areas by concept** — "where does commission logic live," "how is rate limiting implemented," "what does the PDF pipeline look like." Parse the JSON output; `path` and `summary` fields tell you where to look, then read the actual files.

#### Options

| Flag | Description |
|------|-------------|
| `--scope <path>` | Limit to subtree (repeatable: `--scope src/finance --scope src/annuity`) |
| `--filter <glob>` | Glob on result paths (repeatable, OR logic: `--filter "*.go" --filter "*schedule*"`) |
| `--tap <name>` | Limit to tap sources (repeatable: `files`, `linear`, `notion`). Default: all configured taps. (`--provider` is a deprecated alias) |
| `--terse` | Paths + one-sentence summaries, no symbols — minimal context cost |
| `--no-symbols` | File-level results only, omit symbol nesting |
| `-k, --top-k <N>` | Max file results (default 5). Use `-k 2` for precise hits |
| `--symbols-per-file <N>` | Max symbols per file (default 3) |
| `--pretty` | Human-readable colored output instead of JSON |

#### Call graph data

Symbol-level results include `calls` and `called_by` fields when the index has been built with call-graph support. Use these to assess blast radius before making changes:

- `"calls": ["src/finance/rates.rs:lookup_rate_table"]` — this symbol calls `lookup_rate_table` in `src/finance/rates.rs`
- `"called_by": ["src/api/handler.rs:process_request"]` — `process_request` depends on this symbol

A `null` value means the symbol hasn't been indexed with call-graph data yet (run `wdpkr index --skip-summaries` to rebuild). An empty array `[]` means the symbol genuinely has no callers or callees.

When changing a symbol, check its `called_by` to find all dependents — read those files to verify your change doesn't break callers. When exploring unfamiliar code, check `calls` to understand what a function depends on before diving into its implementation.

#### Decay + reinforce (per-tap freshness)

Taps can opt into **time decay** (configured per tap in `config.yaml` under `settings.decay`). When enabled, a result's score is multiplied by `max(floor, 0.5 ^ (age_days / half_life_days))` where age is measured from the document's last index/reinforce time — so stale, unused documents (e.g. old Notion specs) sink in ranking but never drop below `floor` (they stay findable). Decay is a **ranking nudge only**; it never deletes anything. Files typically leave decay off; `notion`/`linear` turn it on.

If a search surfaces a document you actually used, tell wdpkr it was relevant so it stops decaying:

```bash
wdpkr reinforce notion://<page-id>   # bumps last_used_at to now; no re-embedding
```

The id is the result's `path` (e.g. `notion://<page-id>`); the tap is inferred from the URI scheme. This is cheap (a metadata write) — reinforce the specs an agent relied on so the next search ranks them higher.

#### Decision recall (the *why* behind the code)

wdpkr stores **architectural decisions** — authored ADR-style memory that captures why the code is the way it is. They're store-native (a `<namespace>--decision` namespace, not files) and surface in search two ways:

- **`governed_by` on code results** — when a search returns a code file, any active decision whose `areas` glob matches that file is attached: `"governed_by": [{"path": "decision://0007", "title": "Half-up rounding", "status": "accepted"}]`. **Read the governing decision before changing the code it governs.**
- **Direct hits** — decisions appear as their own results with `"source": "decision"` and a `decision://<id>` path. `--tap decision` searches only decisions; `--no-decisions` disables recall for a query.

Record a decision when you make a non-obvious architectural choice, scoping it to the code it governs:

```bash
wdpkr decision add "Half-up rounding for commission" \
  --context "..." --decision "..." --area 'src/finance/**' \
  --tap notion --doc <page-id>   # optionally pull provenance from a tap
```

`--supersedes <id>` retires an old decision (kept, but excluded from active recall); `--overrides <id>` makes a narrow decision win over a broader one in overlapping areas. Manage with `wdpkr decision edit|delete|list`. See `docs/src/content/docs/guides/decisions.md`.

#### When to use

- **Conceptual questions** where you don't know what to grep for: "where does X live," "how is Y implemented"
- **Orientation** before touching an unfamiliar area — get the lay of the land first
- Combine `--scope` with `--filter` and `--terse` for fast, precise lookups:
  `wdpkr search "rate table" --scope src/finance --filter "*.go" --terse -k 3`

#### When NOT to use

- You have a concrete symbol or string to find — use `rg`/grep instead
- You already know which file to read — read it directly
- You need exact text matches or regex — wdpkr is semantic, not lexical

#### Best practices

- **Scope aggressively.** If you know the layer, `--scope` is more valuable than refining the query. Unscoped searches return results across all layers (UI, backend, infra), wasting result slots on irrelevant files.
- **Use `--terse` by default** for simple lookups. Full summaries and symbol trees are useful for deep exploration but waste context tokens when you just need to find the right file.
- **Combine `--scope` with `--filter`** to narrow both the search space and the result set. `--scope` limits the vector query (efficient); `--filter` prunes results by filename pattern (flexible).
- **Switch to `rg` after wdpkr points you somewhere.** Don't chain wdpkr queries to refine — once you have a file or symbol name, grep is faster.
- **Run scoped queries in parallel** when a question spans layers — e.g., one `--scope src/graphql` and one `--scope src/finance`.
