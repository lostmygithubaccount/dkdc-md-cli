use std::io::Read;

use anyhow::{Context, Result, bail};

const ENV_VARS: &[&str] = &[
    "motherduck_token",
    "MOTHERDUCK_TOKEN",
    "motherduck_api_key",
    "MOTHERDUCK_API_KEY",
];

/// Resolve token: CLI flag takes precedence over env vars.
/// Pass `Some("-")` to read from stdin.
pub fn resolve_token_or(cli_token: Option<&str>) -> Result<String> {
    resolve_token_or_with(cli_token, |k| std::env::var(k), std::io::stdin())
}

fn resolve_token_or_with(
    cli_token: Option<&str>,
    env_var: impl Fn(&str) -> Result<String, std::env::VarError>,
    stdin: impl Read,
) -> Result<String> {
    if let Some(token) = cli_token {
        if token == "-" {
            return read_token_from_reader(stdin);
        }
        let trimmed = token.trim();
        anyhow::ensure!(!trimmed.is_empty(), "--token value must not be empty");
        return Ok(trimmed.to_string());
    }
    resolve_token_with(env_var)
}

fn read_token_from_reader(mut reader: impl Read) -> Result<String> {
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .context("failed to read token from stdin")?;
    let trimmed = buf.trim().to_string();
    anyhow::ensure!(!trimmed.is_empty(), "stdin was empty; expected a token");
    Ok(trimmed)
}

fn resolve_token_with(
    env_var: impl Fn(&str) -> Result<String, std::env::VarError>,
) -> Result<String> {
    for var in ENV_VARS {
        if let Ok(val) = env_var(var)
            && !val.is_empty()
        {
            return Ok(val);
        }
    }

    bail!(
        "No MotherDuck token found. Set one of: {}",
        ENV_VARS.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Result<String, std::env::VarError> {
        Err(std::env::VarError::NotPresent)
    }

    fn env_with<'a>(
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Result<String, std::env::VarError> + 'a {
        move |key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .ok_or(std::env::VarError::NotPresent)
        }
    }

    #[test]
    fn resolves_env_vars_in_order() {
        // Only lowest-priority var set
        let env = env_with(&[("MOTHERDUCK_API_KEY", "key4")]);
        assert_eq!(resolve_token_with(env).unwrap(), "key4");

        // Two set — higher priority wins
        let env = env_with(&[
            ("motherduck_api_key", "key3"),
            ("MOTHERDUCK_API_KEY", "key4"),
        ]);
        assert_eq!(resolve_token_with(env).unwrap(), "key3");

        // Three set
        let env = env_with(&[
            ("MOTHERDUCK_TOKEN", "key2"),
            ("motherduck_api_key", "key3"),
            ("MOTHERDUCK_API_KEY", "key4"),
        ]);
        assert_eq!(resolve_token_with(env).unwrap(), "key2");

        // All set — highest priority wins
        let env = env_with(&[
            ("motherduck_token", "key1"),
            ("MOTHERDUCK_TOKEN", "key2"),
            ("motherduck_api_key", "key3"),
            ("MOTHERDUCK_API_KEY", "key4"),
        ]);
        assert_eq!(resolve_token_with(env).unwrap(), "key1");
    }

    #[test]
    fn skips_empty_env_vars() {
        let env = env_with(&[("motherduck_token", ""), ("MOTHERDUCK_TOKEN", "real-token")]);
        assert_eq!(resolve_token_with(env).unwrap(), "real-token");
    }

    #[test]
    fn errors_when_no_token() {
        let err = resolve_token_with(no_env).unwrap_err();
        assert!(err.to_string().contains("No MotherDuck token found"));
    }

    // -- CLI token flag --

    #[test]
    fn cli_token_takes_precedence_over_env() {
        let env = env_with(&[("MOTHERDUCK_TOKEN", "env-tok")]);
        let result = resolve_token_or_with(Some("cli-tok"), env, std::io::empty());
        assert_eq!(result.unwrap(), "cli-tok");
    }

    #[test]
    fn cli_token_trims_whitespace() {
        let result = resolve_token_or_with(Some("  tok  \n"), no_env, std::io::empty());
        assert_eq!(result.unwrap(), "tok");
    }

    #[test]
    fn cli_token_empty_errors() {
        let result = resolve_token_or_with(Some(""), no_env, std::io::empty());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must not be empty")
        );
    }

    #[test]
    fn cli_token_whitespace_only_errors() {
        let result = resolve_token_or_with(Some("   "), no_env, std::io::empty());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must not be empty")
        );
    }

    #[test]
    fn cli_token_dash_reads_stdin() {
        let input = b"stdin-token\n";
        let result = resolve_token_or_with(Some("-"), no_env, &input[..]);
        assert_eq!(result.unwrap(), "stdin-token");
    }

    #[test]
    fn cli_token_dash_trims_stdin() {
        let input = b"  tok-from-pipe  \n";
        let result = resolve_token_or_with(Some("-"), no_env, &input[..]);
        assert_eq!(result.unwrap(), "tok-from-pipe");
    }

    #[test]
    fn cli_token_dash_empty_stdin_errors() {
        let input = b"   \n";
        let result = resolve_token_or_with(Some("-"), no_env, &input[..]);
        assert!(result.unwrap_err().to_string().contains("stdin was empty"));
    }

    #[test]
    fn none_cli_token_falls_through_to_env() {
        let env = env_with(&[("MOTHERDUCK_TOKEN", "env-tok")]);
        let result = resolve_token_or_with(None, env, std::io::empty());
        assert_eq!(result.unwrap(), "env-tok");
    }
}
