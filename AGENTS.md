# AGENTS.md — Multi-agent coordination for wdpkr-core

This file describes how AI agents should interact with this codebase. It supplements `CLAUDE.md` (which covers project-specific conventions) with agent-coordination patterns.

## Who uses this codebase

wdpkr-core is the engine behind wdpkr — built BY AI agents (Claude Code) and FOR AI agents. It has no CLI of its own; its consumers are two other crates:

- **wdpkr** — the CLI, which registers the concrete store backends and glues everything together
- **nidus** — a store backend, which implements this crate's `VectorStore` trait against itself

That makes the **public API a contract between repos**, not an internal detail. A rename here is a breaking change somewhere else, and this crate's CI cannot see it.

## The one invariant

**wdpkr-core must never depend on a concrete store backend** (Turbopuffer, nidus). Storage arrives only through the `VectorStore` trait. `just deps-check` enforces it and runs as part of `just ci`. If a task seems to need a backend here, it belongs in wdpkr.

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

```bash
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file
rm -rf directory            # NOT: rm -r directory
```

## Agent workflow

### Before starting work

```bash
bd prime              # Load beads workflow context
bd ready              # See what's available
bd show <id>          # Read issue details + dependencies
```

### Working on an issue

1. `git checkout main && git pull`
2. `git checkout -b <issue-id>`
3. Implement with tests — run `just ci` before committing
4. If the change touches `.rs`, bump `version` in `Cargo.toml` — every Rust
   change ships a release (see CLAUDE.md § *Every Rust change bumps the
   version*). If the public API changed, also build both consumers against this
   checkout (see below)
5. `git commit` with emoji-prefix message + `Co-Authored-By` trailer
6. `git push -u origin <issue-id>`
7. `gh pr create --fill`
8. `gh pr merge --auto --squash` — hands it to the merge queue, which rebuilds it
   against current `main` and merges when every required check is green
9. `bd close <issue-id>` — nothing auto-closes; a `Closes` line is documentation
10. `just bd-sync` so the issue database follows the code

**`main` is protected — there is no direct push.** The queue is the only way in,
and it never merges red. See CLAUDE.md § *Landing code* for why `merge_group:`
matters when you add **or retire** a required check — the ordering is opposite in
each direction, and getting it backwards stalls every queue entry. `Miri` is not
a required check: it runs on PRs, advisory only, and skips the queue.

**Issue state never rides in git.** The Dolt database under `.beads/` is
gitignored and shared over `refs/dolt/data` on origin, so `bd dolt push` is as
load-bearing as `git push`. In a fresh clone run `just bd-setup` — never `bd init`
or `bd bootstrap`, either of which can leave you with an empty tracker whose
"recovery" force-pushes over everyone's issues.

### Quality gates

Every PR must pass `just ci` which runs:
- `cargo fmt --all -- --check` — formatting
- `cargo clippy --all-targets --all-features -- -D warnings` — linting
- `cargo test --all-features` — all tests (unit + integration)
- `just deps-check` — no store backend in the dependency tree

### Cross-repo changes

A public API change is not done until both consumers compile. Add a patch entry to the consumer's `Cargo.toml`:

```toml
[patch.crates-io]
wdpkr-core = { path = "../wdpkr-core" }
```

then `just check-consumer wdpkr` (and the same for nidus). If a consumer needs updating, file the follow-up issue in *that* repo's beads and link it from this one — do not leave a published breaking change with no landing spot.

### Test philosophy

- **Mock external APIs** — use `src/testing/mock_*.rs` for VectorStore, Embedder, Summarizer, Tap
- **Real tree-sitter** — chunking tests use the actual parser, not mocks
- **Temp git repos** — integration tests create fixture repos in `/tmp`
- **No live API calls in tests** — zero cost, deterministic, CI-safe
- **No backend in tests either** — the trait is exercised through `MockVectorStore`

## Extending the engine

- **New language** → `chunk/languages.rs` + a `tree-sitter-*` dep
- **New embed/summarize provider** → an adapter in `ai_providers/` + a registry entry
- **New tap** → a module in `tap/` implementing the tap trait
- **New store backend** → *not here*. Implement `StoreProvider` + `VectorStore` in the consuming crate.

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
