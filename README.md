# dkdc-md-cli

CLI for the [MotherDuck](https://motherduck.com) REST API.

## install

```bash
# from source (Rust binary)
cargo install --path dkdc-md-cli

# from source (Python)
uv tool install .
```

## authentication

Set one of the following environment variables:

```bash
export MOTHERDUCK_TOKEN="your-token-here"
```

Also accepted: `motherduck_token`, `MOTHERDUCK_API_KEY`, `motherduck_api_key`.

## usage

```bash
# service accounts
md service-account create myaccount
md service-account delete myaccount

# duckling configuration
md duckling get myaccount
md duckling set myaccount --rw-size pulse --rs-size pulse --flock-size 1

# access tokens
md token list myaccount
md token create myaccount --name my-token --ttl 3600
md token delete myaccount <token-id>

# active accounts
md account list-active

# JSON output (for piping to jq, etc.)
md token list myaccount -o json
```

## development

```bash
bin/setup     # install rustup + uv
bin/build     # build Rust + Python
bin/check     # lint + test
bin/format    # auto-format
bin/install   # install locally
```

Integration tests (requires `MOTHERDUCK_TOKEN`):

```bash
tests/integration-test
```
