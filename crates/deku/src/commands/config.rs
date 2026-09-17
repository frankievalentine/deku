use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    /// List all config vars for an app
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Set one or more config vars (KEY=VALUE ...)
    Set {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "KEY=VALUE pairs", num_args = 1..)]
        pairs: Vec<String>,
    },
    /// Unset a config var
    Unset {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Key name")]
        key: String,
    },
    /// Import config vars from a .env file
    Import {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, help = "Path to the .env file")]
        file: std::path::PathBuf,
        #[arg(long, help = "Replace vars that already exist")]
        overwrite: bool,
    },
}

pub async fn run(args: ConfigArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ConfigCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/config")).await?;
            let empty = Vec::new();
            let vars = data.as_array().unwrap_or(&empty);
            if vars.is_empty() {
                println!("No config vars set.");
            } else {
                for var in vars {
                    let key = var["key"].as_str().unwrap_or("");
                    if key.is_empty() {
                        continue;
                    }
                    // An encrypted value with no usable key must not look empty.
                    let value = match var["error"].as_str() {
                        Some(error) => format!("<unreadable: {error}>"),
                        None => var["value"].as_str().unwrap_or("").to_string(),
                    };
                    let locked = if var["encrypted"].as_bool().unwrap_or(false) {
                        " [encrypted]"
                    } else {
                        ""
                    };
                    match var["is_global"].as_bool().unwrap_or(false) {
                        true => println!("{key}={value}  (global){locked}"),
                        false => println!("{key}={value}{locked}"),
                    }
                }
            }
        }

        ConfigCommands::Set { app, pairs } => {
            for pair in &pairs {
                let (k, v) = pair
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("invalid KEY=VALUE: {pair}"))?;
                client
                    .post(
                        &format!("/api/apps/{app}/config"),
                        serde_json::json!({
                            "key": k,
                            "value": v,
                        }),
                    )
                    .await?;
            }
            println!("Config vars set for '{app}'.");
        }

        ConfigCommands::Unset { app, key } => {
            client
                .delete(&format!("/api/apps/{app}/config/{key}"))
                .await?;
            println!("Unset '{key}' for '{app}'.");
        }

        ConfigCommands::Import {
            app,
            file,
            overwrite,
        } => {
            let contents = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", file.display()))?;
            let vars = parse_env_file(&contents)?;
            if vars.is_empty() {
                println!("No vars found in {}.", file.display());
                return Ok(());
            }

            let data = client
                .post(
                    &format!("/api/apps/{app}/config/import"),
                    serde_json::json!({ "vars": vars, "overwrite": overwrite }),
                )
                .await?;

            let created = data["created"].as_u64().unwrap_or(0);
            let overwritten = data["overwritten"].as_u64().unwrap_or(0);
            println!(
                "Imported {} vars into '{app}': {created} added, {overwritten} updated.",
                vars.len()
            );

            if let Some(skipped) = data["skipped"].as_array() {
                if !skipped.is_empty() {
                    let keys: Vec<&str> = skipped.iter().filter_map(|k| k.as_str()).collect();
                    println!("Skipped {} already set: {}", keys.len(), keys.join(", "));
                    println!("Re-run with --overwrite to replace them.");
                }
            }
        }
    }
    Ok(())
}

/// Parse a `.env` file into key/value pairs.
///
/// Supported syntax: blank lines, `#` comments, an optional `export` prefix,
/// `KEY=VALUE`, and single- or double-quoted values. Double quotes expand `\n`,
/// `\t`, `\r`, `\"`, and `\\`. For unquoted values, an inline ` #` comment is
/// stripped.
fn parse_env_file(contents: &str) -> Result<std::collections::BTreeMap<String, String>> {
    let mut vars = std::collections::BTreeMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(anyhow::anyhow!(
                "line {line_number}: expected KEY=VALUE, found {raw_line:?}"
            ));
        };

        let key = raw_key.trim();
        if key.is_empty()
            || !key
                .chars()
                .enumerate()
                .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
        {
            return Err(anyhow::anyhow!(
                "line {line_number}: invalid key {key:?}; use letters, digits, and underscores, and do not start with a digit"
            ));
        }

        let value = parse_env_value(raw_value.trim());
        // A later line wins, matching common .env behaviour.
        vars.insert(key.to_string(), value);
    }

    Ok(vars)
}

fn parse_env_value(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return inner.to_string();
    }

    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return inner
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }

    // Strip an inline comment only when it follows whitespace, so values that
    // legitimately contain '#' are left alone.
    match value.find(" #") {
        Some(index) => value[..index].trim_end().to_string(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_env_file;

    #[test]
    fn parses_basic_comments_and_blanks() {
        let vars = parse_env_file("# comment\n\nNODE_ENV=production\nPORT=3000\n").expect("parse");
        assert_eq!(vars.get("NODE_ENV").map(String::as_str), Some("production"));
        assert_eq!(vars.get("PORT").map(String::as_str), Some("3000"));
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn strips_export_prefix_and_surrounding_whitespace() {
        let vars = parse_env_file("export   API_URL = https://example.test \n").expect("parse");
        assert_eq!(
            vars.get("API_URL").map(String::as_str),
            Some("https://example.test")
        );
    }

    #[test]
    fn handles_quoting() {
        let vars = parse_env_file(
            "A='literal $HOME # not a comment'\nB=\"line1\\nline2\"\nC=\"a \\\"quote\\\"\"\n",
        )
        .expect("parse");
        assert_eq!(
            vars.get("A").map(String::as_str),
            Some("literal $HOME # not a comment")
        );
        assert_eq!(vars.get("B").map(String::as_str), Some("line1\nline2"));
        assert_eq!(vars.get("C").map(String::as_str), Some("a \"quote\""));
    }

    #[test]
    fn strips_whitespace_delimited_inline_comments() {
        let vars = parse_env_file("PORT=3000 # the web port\nHASH=a#b\n").expect("parse");
        assert_eq!(vars.get("PORT").map(String::as_str), Some("3000"));
        assert_eq!(vars.get("HASH").map(String::as_str), Some("a#b"));
    }

    #[test]
    fn empty_values_are_allowed_and_last_duplicate_wins() {
        let vars = parse_env_file("EMPTY=\nKEY=first\nKEY=second\n").expect("parse");
        assert_eq!(vars.get("EMPTY").map(String::as_str), Some(""));
        assert_eq!(vars.get("KEY").map(String::as_str), Some("second"));
    }

    #[test]
    fn rejects_malformed_lines_with_line_numbers() {
        let error = parse_env_file("GOOD=1\nNOT_A_PAIR\n").expect_err("should fail");
        assert!(error.to_string().contains("line 2"), "{error}");

        let error = parse_env_file("1BAD=x\n").expect_err("should fail");
        assert!(error.to_string().contains("invalid key"), "{error}");
    }
}
