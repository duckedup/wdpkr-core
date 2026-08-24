#!/usr/bin/env bash
# Assert wdpkr-core's one invariant: no concrete vector-store backend in the
# dependency tree. Storage arrives only through the `VectorStore` trait, which
# this crate defines and never implements — a dep on nidus would also be a
# dependency cycle, since nidus imports this crate to implement that trait.
#
# A script rather than an inline recipe so `just deps-check` and the CI job run
# the SAME logic — one source of truth, shared with .github/workflows/ci.yml so
# local and CI cannot drift (and so CI needs no `just` on the runner).
set -euo pipefail

FORBIDDEN=(nidus turbopuffer)

tree=$(cargo tree --prefix none --format '{p}' --all-features)

failed=0
for crate in "${FORBIDDEN[@]}"; do
    if grep -qE "^${crate} v" <<<"$tree"; then
        echo "error: wdpkr-core must not depend on '$crate' — it is a store backend." >&2
        echo "       Backends register through StoreProvider from the consuming crate." >&2
        failed=1
    fi
done

if [ "$failed" -ne 0 ]; then
    exit 1
fi

echo "ok: no store backend in the dependency tree"
