# The store seam

This module is the reason wdpkr-core exists as a separate crate. It defines
`VectorStore` (runtime ops) and `StoreProvider` (registration + config) and
**implements neither**. `scripts/deps-check.sh` fails the build if a concrete
backend (`nidus`, `turbopuffer`) ever enters the dependency tree.

## Why the trait lives here and not in wdpkr

```
        wdpkr-core          ← traits only
        ↗         ↖
    wdpkr    →    nidus
```

`wdpkr` depends on `nidus` (it ships a nidus-backed store). If the trait lived
in `wdpkr`, then `nidus` implementing it would close the loop
`nidus → wdpkr → nidus`. Putting the trait in a crate that depends on neither
is what lets nidus offer its own `vectorize`-style command using this engine.

**So: never add a backend here.** If you find yourself wanting one, the code
belongs in the consuming crate.

## Implementing a backend (from a consuming crate)

1. `impl VectorStore` — the runtime operations.
2. `impl StoreProvider` — `name()`, `settings()`, `validate()`, `build()`.
3. Call `register_provider(Arc::new(YourProvider))` **before** resolving
   config.

That ordering matters: `StoreConfig::resolve` reads every registered
provider's `settings()` to know which env vars and defaults exist. A backend
registered afterwards gets no settings resolved, silently.

## Config is opaque by design

Core cannot name a backend, so it does not hold typed per-backend structs.
`StoreConfig` is `provider` plus a bag keyed `<provider>.<key>`, and each
provider declares its own keys via `SettingSpec`:

```rust
fn settings(&self) -> &'static [SettingSpec] {
    &[SettingSpec {
        key: "path",                    // -> store.myback.path
        env: "MYBACK_PATH",             // env layer
        secret: false,                  // true withholds it from `config list`
        default: default_myback_path,   // computed, not a literal
        file_aliases: &[],              // deprecated flat keys, read-only
    }]
}
```

Core then runs the same defaults → file → env chain it uses for every other
setting and records `Source` attribution — which is how `wdpkr config list`
prints `store.myback.path` sourced from `MYBACK_PATH` while this crate has
never heard of `myback`. Read settings with `config.get("myback.path")`.

`validate()` is deliberately separate from `build()`: `wdpkr config` reports a
missing credential, and the indexer fails fast, without opening a connection.

There is **no default provider**. Core will not prefer a backend it does not
know, so an unset `store.provider` is a `validate()` error listing what is
registered. A consumer wanting a zero-config default supplies it in its own
config layer.

## Backend-specific adapter rules

Live with the backend, not here — see `wdpkr`'s `src/store/CLAUDE.md` for the
turbopuffer and nidus adapters (filter pushdown, score semantics, attribute
encoding, batching).
