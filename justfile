# Lurkn dev entrypoints. `just` with no argument lists them.
default:
    @just --list

# Run the app with hot reload.
dev:
    pnpm tauri dev

# Everything CI runs, in CI order.
ci: fmt-check lint test guard-no-html

# Rust + frontend tests. Regenerates src/lib/ipc/bindings.ts as a side effect.
test:
    cargo test --workspace
    pnpm test

fmt:
    cargo fmt --all
    pnpm exec prettier --write . 2>/dev/null || true

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm lint
    pnpm check

# docs/architecture.md §11: a lint rule can be disabled inline, a grep cannot.
guard-no-html:
    #!/usr/bin/env bash
    set -euo pipefail
    if grep -rn --include='*.svelte' --include='*.ts' -E '\{@html|innerHTML|outerHTML|insertAdjacentHTML' src/; then
        echo "error: raw HTML injection found in src/ — see docs/architecture.md §11" >&2
        exit 1
    fi
    echo "ok: no raw HTML injection in src/"

# Contract checks against the real 7TV / BTTV / FFZ APIs. Manual on purpose: a
# provider outage must never red the build, so CI does not run these.
live-check:
    cargo test -p lurkn-emotes --test live -- --ignored --nocapture

# Re-capture API fixtures from live services. Manual, for the same reason.
refresh-fixtures:
    @echo "Fixtures are inline in the provider tests; run 'just live-check' to verify they still match."

build:
    pnpm tauri build

# Prints the git commands to tag the release; runs none of them (docs/release.md).
# Set a new version everywhere and date its changelog section.
release VERSION:
    node scripts/version.mjs {{VERSION}}

# Regenerate THIRD_PARTY_LICENSES.md from the dependency tree (needs cargo-about).
licenses:
    cargo about generate about.hbs -o THIRD_PARTY_LICENSES.md

# Licenses, advisories, bans and sources of the dependency tree (needs cargo-deny).
deny:
    cargo deny check
