use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::auth;
use crate::client::MotherduckClient;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum OutputMode {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum InstanceSize {
    Pulse,
    Standard,
    Jumbo,
    Mega,
    Giga,
}

impl InstanceSize {
    fn as_api_str(&self) -> &'static str {
        match self {
            Self::Pulse => "pulse",
            Self::Standard => "standard",
            Self::Jumbo => "jumbo",
            Self::Mega => "mega",
            Self::Giga => "giga",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TokenType {
    ReadWrite,
    ReadScaling,
}

impl TokenType {
    fn as_api_str(&self) -> &'static str {
        match self {
            Self::ReadWrite => "read_write",
            Self::ReadScaling => "read_scaling",
        }
    }
}

#[derive(Parser)]
#[command(name = "md", version, about = "CLI for the MotherDuck REST API")]
struct Cli {
    /// Output format
    #[arg(short, long, global = true, value_enum, default_value_t = OutputMode::Text)]
    output: OutputMode,

    /// API token (overrides env vars; use '-' to read from stdin)
    #[arg(long, global = true)]
    token: Option<String>,

    /// Skip confirmation prompts
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage service accounts
    ServiceAccount {
        #[command(subcommand)]
        command: ServiceAccountCommands,
    },
    /// Manage access tokens
    Token {
        #[command(subcommand)]
        command: TokenCommands,
    },
    /// Manage duckling configuration
    Duckling {
        #[command(subcommand)]
        command: DucklingCommands,
    },
    /// Manage accounts
    Account {
        #[command(subcommand)]
        command: AccountCommands,
    },
}

#[derive(Subcommand)]
enum ServiceAccountCommands {
    /// Create a new service account
    Create {
        /// Username
        username: String,
    },
    /// Delete a service account
    Delete {
        /// Username
        username: String,
    },
}

#[derive(Subcommand)]
enum TokenCommands {
    /// List tokens for a user
    List {
        /// Username
        username: String,
    },
    /// Create a new access token
    Create {
        /// Username
        username: String,
        /// Token name
        #[arg(short, long)]
        name: String,
        /// Time-to-live in seconds (300-31536000)
        #[arg(long, value_parser = clap::value_parser!(u64).range(300..=31536000))]
        ttl: Option<u64>,
        /// Token type
        #[arg(long, value_enum, default_value_t = TokenType::ReadWrite)]
        token_type: TokenType,
    },
    /// Delete an access token
    Delete {
        /// Username
        username: String,
        /// Token ID
        token_id: String,
    },
}

#[derive(Subcommand)]
enum DucklingCommands {
    /// Get duckling configuration for a user
    Get {
        /// Username
        username: String,
    },
    /// Set duckling configuration for a user (fetches current config, merges overrides)
    #[command(group(clap::ArgGroup::new("overrides").required(true).multiple(true)))]
    Set {
        /// Username
        username: String,
        /// Read-write instance size
        #[arg(long, value_enum, group = "overrides")]
        rw_size: Option<InstanceSize>,
        /// Read-scaling instance size
        #[arg(long, value_enum, group = "overrides")]
        rs_size: Option<InstanceSize>,
        /// Read-scaling flock size (0-64)
        #[arg(long, group = "overrides", value_parser = clap::value_parser!(u32).range(0..=64))]
        flock_size: Option<u32>,
    },
}

#[derive(Subcommand)]
enum AccountCommands {
    /// List active accounts
    ListActive,
}

// -- helpers --

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("Value serialization is infallible")
    );
}

/// Extract a string field for display. Returns "-" for missing/null fields.
fn display_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("-")
}

/// Extract a string field for use as data. Returns None for missing/null fields.
fn extract_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str()
}

/// Print rows as a fixed-width table with a header.
fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let widths: Vec<usize> = (0..headers.len())
        .map(|i| {
            let header_w = headers[i].len();
            let max_row_w = rows
                .iter()
                .map(|r| r.get(i).map_or(0, |s| s.len()))
                .max()
                .unwrap_or(0);
            header_w.max(max_row_w)
        })
        .collect();

    let last = headers.len() - 1;

    // Header
    for (i, h) in headers.iter().enumerate() {
        if i < last {
            print!("{:<width$}  ", h, width = widths[i]);
        } else {
            println!("{h}");
        }
    }

    // Rows
    for row in rows {
        for (i, val) in row.iter().enumerate() {
            if i < last {
                print!("{:<width$}  ", val, width = widths[i]);
            } else {
                println!("{val}");
            }
        }
    }
}

fn print_duckling_config(value: &Value) {
    let rw = display_field(&value["read_write"], "instance_size");
    let rs = display_field(&value["read_scaling"], "instance_size");
    let flock = match value["read_scaling"]["flock_size"].as_u64() {
        Some(n) => n.to_string(),
        None => "-".to_string(),
    };
    println!("read_write:   {rw}");
    println!("read_scaling: {rs} (flock_size: {flock})");
}

/// Ask the user for confirmation on stderr. Returns Ok(()) if confirmed, Err if declined.
/// Auto-confirms if `--yes` was passed or if stdin is not a terminal.
fn confirm(prompt: &str, yes: bool) -> Result<()> {
    if yes || !std::io::stdin().is_terminal() {
        return Ok(());
    }
    eprint!("{prompt}");
    std::io::stderr()
        .flush()
        .context("failed to flush stderr")?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;
    let answer = input.trim().to_lowercase();
    if answer == "y" || answer == "yes" {
        Ok(())
    } else {
        bail!("aborted")
    }
}

// -- command handlers --

fn handle_service_account(
    client: &MotherduckClient,
    command: ServiceAccountCommands,
    mode: OutputMode,
    yes: bool,
) -> Result<()> {
    match command {
        ServiceAccountCommands::Create { username } => {
            let result = client.create_user(&username)?;
            match mode {
                OutputMode::Json => print_json(&result),
                OutputMode::Text => println!("{}", display_field(&result, "username")),
            }
        }
        ServiceAccountCommands::Delete { username } => {
            confirm(&format!("Delete service account '{username}'? [y/N] "), yes)?;
            let result = client.delete_user(&username)?;
            if mode == OutputMode::Json {
                print_json(&result);
            }
        }
    }
    Ok(())
}

fn handle_token(
    client: &MotherduckClient,
    command: TokenCommands,
    mode: OutputMode,
    yes: bool,
) -> Result<()> {
    match command {
        TokenCommands::List { username } => {
            let result = client.list_tokens(&username)?;
            match mode {
                OutputMode::Json => print_json(&result),
                OutputMode::Text => {
                    if let Some(tokens) = result["tokens"].as_array() {
                        let rows: Vec<Vec<String>> = tokens
                            .iter()
                            .map(|t| {
                                vec![
                                    display_field(t, "id").to_string(),
                                    display_field(t, "name").to_string(),
                                    display_field(t, "token_type").to_string(),
                                    match t["expire_at"].as_str() {
                                        Some(s) if !s.is_empty() => s.to_string(),
                                        _ => "never".to_string(),
                                    },
                                ]
                            })
                            .collect();
                        print_table(&["ID", "NAME", "TYPE", "EXPIRES"], &rows);
                    }
                }
            }
        }
        TokenCommands::Create {
            username,
            name,
            ttl,
            token_type,
        } => {
            let result =
                client.create_token(&username, &name, ttl, Some(token_type.as_api_str()))?;
            match mode {
                OutputMode::Json => print_json(&result),
                OutputMode::Text => println!("{}", display_field(&result, "token")),
            }
        }
        TokenCommands::Delete { username, token_id } => {
            confirm(&format!("Delete token '{token_id}'? [y/N] "), yes)?;
            let result = client.delete_token(&username, &token_id)?;
            if mode == OutputMode::Json {
                print_json(&result);
            }
        }
    }
    Ok(())
}

fn handle_duckling(
    client: &MotherduckClient,
    command: DucklingCommands,
    mode: OutputMode,
) -> Result<()> {
    let result = match command {
        DucklingCommands::Get { username } => client.get_duckling_config(&username)?,
        DucklingCommands::Set {
            username,
            rw_size,
            rs_size,
            flock_size,
        } => {
            let current = client.get_duckling_config(&username)?;
            let rw = match rw_size {
                Some(s) => s.as_api_str(),
                None => extract_str(&current["read_write"], "instance_size")
                    .context("current config missing read_write.instance_size")?,
            };
            let rs = match rs_size {
                Some(s) => s.as_api_str(),
                None => extract_str(&current["read_scaling"], "instance_size")
                    .context("current config missing read_scaling.instance_size")?,
            };
            let flock = match flock_size {
                Some(n) => n,
                None => current["read_scaling"]["flock_size"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .context("current config missing read_scaling.flock_size")?,
            };
            client.set_duckling_config(&username, rw, rs, flock)?
        }
    };
    match mode {
        OutputMode::Json => print_json(&result),
        OutputMode::Text => print_duckling_config(&result),
    }
    Ok(())
}

fn handle_account(
    client: &MotherduckClient,
    command: AccountCommands,
    mode: OutputMode,
) -> Result<()> {
    match command {
        AccountCommands::ListActive => {
            let result = client.list_active_accounts()?;
            match mode {
                OutputMode::Json => print_json(&result),
                OutputMode::Text => {
                    if let Some(accounts) = result["accounts"].as_array() {
                        let rows: Vec<Vec<String>> = accounts
                            .iter()
                            .map(|acct| {
                                let username = display_field(acct, "username").to_string();
                                let ducklings = acct["ducklings"]
                                    .as_array()
                                    .map(|ds| {
                                        ds.iter()
                                            .map(|d| {
                                                format!(
                                                    "{} ({})",
                                                    display_field(d, "type"),
                                                    display_field(d, "status"),
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_default();
                                vec![username, ducklings]
                            })
                            .collect();
                        print_table(&["USERNAME", "DUCKLINGS"], &rows);
                    }
                }
            }
        }
    }
    Ok(())
}

// -- main dispatch --

/// Parse CLI arguments and execute the corresponding MotherDuck API command.
pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let mode = cli.output;
    let yes = cli.yes;
    let token = auth::resolve_token_or(cli.token.as_deref())?;
    let client = MotherduckClient::new(&token)?;

    match cli.command {
        Commands::ServiceAccount { command } => handle_service_account(&client, command, mode, yes),
        Commands::Token { command } => handle_token(&client, command, mode, yes),
        Commands::Duckling { command } => handle_duckling(&client, command, mode),
        Commands::Account { command } => handle_account(&client, command, mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    // -- InstanceSize / TokenType --

    #[test]
    fn instance_size_as_api_str() {
        assert_eq!(InstanceSize::Pulse.as_api_str(), "pulse");
        assert_eq!(InstanceSize::Standard.as_api_str(), "standard");
        assert_eq!(InstanceSize::Jumbo.as_api_str(), "jumbo");
        assert_eq!(InstanceSize::Mega.as_api_str(), "mega");
        assert_eq!(InstanceSize::Giga.as_api_str(), "giga");
    }

    #[test]
    fn token_type_as_api_str() {
        assert_eq!(TokenType::ReadWrite.as_api_str(), "read_write");
        assert_eq!(TokenType::ReadScaling.as_api_str(), "read_scaling");
    }

    // -- CLI parsing --

    #[test]
    fn parse_service_account_create() {
        let cli = parse(&["md", "service-account", "create", "svc_test"]).unwrap();
        match cli.command {
            Commands::ServiceAccount {
                command: ServiceAccountCommands::Create { username },
            } => assert_eq!(username, "svc_test"),
            _ => panic!("expected ServiceAccount Create"),
        }
    }

    #[test]
    fn parse_service_account_delete() {
        let cli = parse(&["md", "service-account", "delete", "svc_test"]).unwrap();
        match cli.command {
            Commands::ServiceAccount {
                command: ServiceAccountCommands::Delete { username },
            } => assert_eq!(username, "svc_test"),
            _ => panic!("expected ServiceAccount Delete"),
        }
    }

    #[test]
    fn parse_token_create_all_options() {
        let cli = parse(&[
            "md",
            "token",
            "create",
            "svc_test",
            "--name",
            "my-tok",
            "--ttl",
            "3600",
            "--token-type",
            "read-scaling",
        ])
        .unwrap();
        match cli.command {
            Commands::Token {
                command:
                    TokenCommands::Create {
                        username,
                        name,
                        ttl,
                        token_type,
                    },
            } => {
                assert_eq!(username, "svc_test");
                assert_eq!(name, "my-tok");
                assert_eq!(ttl.unwrap(), 3600);
                assert_eq!(token_type.as_api_str(), "read_scaling");
            }
            _ => panic!("expected Token Create"),
        }
    }

    #[test]
    fn parse_token_create_defaults() {
        let cli = parse(&["md", "token", "create", "u", "--name", "t"]).unwrap();
        match cli.command {
            Commands::Token {
                command:
                    TokenCommands::Create {
                        ttl, token_type, ..
                    },
            } => {
                assert!(ttl.is_none());
                assert_eq!(token_type.as_api_str(), "read_write");
            }
            _ => panic!("expected Token Create"),
        }
    }

    #[test]
    fn parse_token_create_missing_name_fails() {
        assert!(parse(&["md", "token", "create", "u"]).is_err());
    }

    #[test]
    fn parse_invalid_instance_size_fails() {
        assert!(parse(&["md", "duckling", "set", "u", "--rw-size", "tiny"]).is_err());
    }

    #[test]
    fn parse_duckling_set_requires_at_least_one_override() {
        assert!(parse(&["md", "duckling", "set", "u"]).is_err());
    }

    #[test]
    fn parse_global_output_flag() {
        let cli = parse(&["md", "-o", "json", "account", "list-active"]).unwrap();
        assert_eq!(cli.output, OutputMode::Json);
    }

    #[test]
    fn parse_default_output_is_text() {
        let cli = parse(&["md", "account", "list-active"]).unwrap();
        assert_eq!(cli.output, OutputMode::Text);
    }

    // -- --token flag --

    #[test]
    fn parse_global_token_flag() {
        let cli = parse(&["md", "--token", "my-secret", "account", "list-active"]).unwrap();
        assert_eq!(cli.token.as_deref(), Some("my-secret"));
    }

    #[test]
    fn parse_token_flag_defaults_to_none() {
        let cli = parse(&["md", "account", "list-active"]).unwrap();
        assert!(cli.token.is_none());
    }

    #[test]
    fn parse_token_dash_for_stdin() {
        let cli = parse(&["md", "--token", "-", "account", "list-active"]).unwrap();
        assert_eq!(cli.token.as_deref(), Some("-"));
    }

    // -- --yes flag --

    #[test]
    fn parse_yes_short_flag() {
        let cli = parse(&["md", "-y", "service-account", "delete", "svc_test"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn parse_yes_long_flag() {
        let cli = parse(&["md", "--yes", "token", "delete", "u", "t123"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn parse_yes_after_args() {
        let cli = parse(&["md", "service-account", "delete", "svc_test", "--yes"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn parse_token_after_args() {
        let cli = parse(&["md", "account", "list-active", "--token", "tok"]).unwrap();
        assert_eq!(cli.token.as_deref(), Some("tok"));
    }

    #[test]
    fn parse_yes_defaults_to_false() {
        let cli = parse(&["md", "account", "list-active"]).unwrap();
        assert!(!cli.yes);
    }

    // -- helpers --

    #[test]
    fn display_field_returns_value() {
        let v = serde_json::json!({"name": "alice"});
        assert_eq!(display_field(&v, "name"), "alice");
    }

    #[test]
    fn display_field_returns_dash_for_missing() {
        let v = serde_json::json!({});
        assert_eq!(display_field(&v, "name"), "-");
    }

    #[test]
    fn print_table_empty_rows_no_output() {
        // Should not panic or print anything
        print_table(&["A", "B"], &[]);
    }

    #[test]
    fn print_table_single_row() {
        print_table(&["A", "B"], &[vec!["short".into(), "x".into()]]);
    }

    #[test]
    fn print_table_varying_widths() {
        print_table(
            &["ID", "NAME"],
            &[
                vec!["1".into(), "alice".into()],
                vec!["1000".into(), "b".into()],
            ],
        );
    }
}
