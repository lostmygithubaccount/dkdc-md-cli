# dkdc-md-cli

CLI for the [MotherDuck REST API](https://motherduck.com/docs/sql-reference/rest-api/motherduck-rest-api/).

## Install

Pre-built binaries are available for Linux and macOS via Python. Windows users should install via `cargo` or use macOS/Linux.

uv:

```bash
uv tool install dkdc-md-cli
```

uvx:

```bash
uvx --from dkdc-md-cli md
```

cargo:

```bash
cargo install dkdc-md-cli
```

## Authentication

Set a MotherDuck API token via environment variable:

```bash
export MOTHERDUCK_TOKEN="your-token-here"
```

Token resolution order (first non-empty wins):

1. `--token` flag (pass `-` to read from stdin)
2. `motherduck_token`
3. `MOTHERDUCK_TOKEN`
4. `motherduck_api_key`
5. `MOTHERDUCK_API_KEY`

## Usage

```
md [--output text|json] [--token TOKEN] [--yes] <command>
```

### Global flags

| Flag | Short | Description |
|------|-------|-------------|
| `--output` | `-o` | Output format: `text` (default) or `json` |
| `--token` | | API token (overrides env vars; `-` reads from stdin) |
| `--yes` | `-y` | Skip confirmation prompts |

### `service-account`

```bash
# Create a service account
md service-account create <username>

# Delete a service account (prompts for confirmation)
md service-account delete <username>
```

### `token`

```bash
# List tokens for a user
md token list <username>

# Create a new token
md token create <username> --name <name> [--ttl <seconds>] [--token-type <type>]

# Delete a token (prompts for confirmation)
md token delete <username> <token_id>
```

`--ttl`: time-to-live in seconds (300–31536000). Omit for no expiration.

`--token-type`: `read-write` (default) or `read-scaling`.

### `duckling`

```bash
# Get current duckling config
md duckling get <username>

# Set duckling config (at least one override required)
md duckling set <username> [--rw-size <size>] [--rs-size <size>] [--flock-size <n>]
```

Instance sizes: `pulse`, `standard`, `jumbo`, `mega`, `giga`.

Flock size: 0–64. `duckling set` fetches the current config and merges your overrides, so you only need to specify what you're changing.

### `account`

```bash
# List active accounts and their ducklings
md account list-active
```
