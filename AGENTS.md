# dkdc-md-cli

CLI for the MotherDuck REST API. Rust core with PyO3 Python bindings.

**Standalone project.** This lives in the dkdc monorepo for convenience but is entirely disconnected from the rest of the monorepo. It has no dependencies on any dkdc crate, is not part of the dkdc Cargo workspace, and is not a sub-CLI of `dkdc`. It is synced to its own public repo and published independently.

## architecture

```
dkdc-md-cli/            # pure Rust core (lib + binary)
  src/
    lib.rs              # module exports, pub fn run()
    main.rs             # binary entry point
    cli.rs              # clap CLI definition + command dispatch
    client.rs           # ureq HTTP client for MotherDuck API
    auth.rs             # token resolution (env vars)
dkdc-md-cli-py/         # PyO3 cdylib bindings (own workspace, built by maturin)
  src/lib.rs            # single run() function exposed as dkdc_md.core
src/
  dkdc_md/
    __init__.py         # thin Python wrapper, entry point for `uv tool install .`
```

The `-py` crate is **not** in the Cargo workspace (cdylib can't link Python symbols via `cargo build`). It's built exclusively by maturin.

Crates.io: `dkdc-md-cli`. PyPI: `dkdc-md-cli`. Installed binary: `md`.

## development

```bash
bin/setup       # install rustup + uv if needed
bin/build       # build Rust + Python (bin/build-rs, bin/build-py)
bin/check       # lint + test (bin/check-rs, bin/check-py)
bin/format      # auto-format (bin/format-rs, bin/format-py)
bin/test        # run tests (bin/test-rs, bin/test-py)
bin/install     # install locally (bin/install-rs, bin/install-py)
```

Rust checks: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
Python checks: `ruff check .`, `ruff format --check .`

## testing

Integration test against the live MotherDuck API (requires `MOTHERDUCK_TOKEN`):

```bash
tests/integration-test
```

This creates a temporary service account, exercises duckling config (pulse -> standard -> pulse), creates/lists/deletes tokens, then cleans up. Uses cleanup trap for safety.

## authentication

Token resolution order (first non-empty wins):
1. `motherduck_token` env var
2. `MOTHERDUCK_TOKEN` env var
3. `motherduck_api_key` env var
4. `MOTHERDUCK_API_KEY` env var

## CLI reference

```
md [-o text|json] [-V]

  service-account create <username>
  service-account delete <username>

  token list <username>
  token create <username> --name NAME [--ttl SECS] [--token-type read-write|read-scaling]
  token delete <username> <token_id>

  duckling get <username>
  duckling set <username> --rw-size SIZE --rs-size SIZE --flock-size N

  account list-active
```

Instance sizes are validated client-side via clap ValueEnum: `pulse`, `standard`, `jumbo`, `mega`, `giga`.

## MotherDuck API reference

OpenAPI spec: https://api.motherduck.com/docs/specs

## CI/CD

Public repo: `lostmygithubaccount/dkdc-md-cli`

- `.github/workflows/ci.yml` — runs checks on push/PR to main
- `.github/workflows/check.yml` — reusable workflow (fmt, clippy, test, ruff)
- `.github/workflows/release.yml` — multi-platform Rust binaries on version tags
- `.github/workflows/release-python.yml` — PyPI wheels + sdist on version tags

Release: tag `v*.*.*` triggers both workflows. PyPI uses OIDC trusted publishing.

Sync to public repo: `bin/sync-public-repo` (optional `--release` flag to tag).

## conventions

- Rust stable toolchain (edition 2024, requires 1.93+)
- All API methods return `serde_json::Value` (thin wrapper, not typed responses)
- `handle_response()` reads body as text first, then tries JSON parse (robust against non-JSON errors)
- `service-account create` uses API defaults (standard, flock_size=4). Use `duckling set` to override config after creation.
