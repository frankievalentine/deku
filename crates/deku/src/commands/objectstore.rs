use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use deku_core::types::ObjectStoreConfig;

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct ObjectStoreArgs {
    #[command(subcommand)]
    command: ObjectStoreCommands,
}

#[derive(Debug, Subcommand)]
enum ObjectStoreCommands {
    /// Configure an S3-compatible object store
    Setup {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        bucket: Option<String>,
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        access_key_id: Option<String>,
        #[arg(long)]
        secret_access_key: Option<String>,
        #[arg(long)]
        prefix: Option<String>,
        #[arg(long)]
        path_style: bool,
        #[arg(long)]
        virtual_host_style: bool,
        #[arg(long)]
        no_test: bool,
    },
    /// Show current object store configuration
    Info,
    /// Run a write/read/delete connectivity check
    Test,
    /// Remove the current object store configuration
    Unset,
    /// Show the object-store link status for an app
    Status { app: String },
    /// Link the configured object store credentials into an app
    Link {
        app: String,
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Remove the object-store credentials from an app
    Unlink { app: String },
}

pub async fn run(args: ObjectStoreArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ObjectStoreCommands::Setup {
            provider,
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            prefix,
            path_style,
            virtual_host_style,
            no_test,
        } => {
            if path_style && virtual_host_style {
                return Err(anyhow!(
                    "use either --path-style or --virtual-host-style, not both"
                ));
            }

            let provider = prompt_required("Provider", provider, "r2")?;
            let bucket = prompt_required("Bucket", bucket, "deku")?;
            let endpoint = prompt_required("Endpoint", endpoint, "")?;
            let default_region = if provider == "r2" {
                "auto"
            } else {
                "us-east-1"
            };
            let region = prompt_required("Region", region, default_region)?;
            let access_key_id = prompt_required("Access key ID", access_key_id, "")?;
            let secret_access_key = prompt_required("Secret access key", secret_access_key, "")?;
            let prefix = prompt_optional("Prefix (leave blank to skip)", prefix)?;

            let detected_path_style = if path_style {
                true
            } else if virtual_host_style {
                false
            } else {
                provider == "r2"
            };

            let config = ObjectStoreConfig {
                provider,
                bucket,
                region,
                endpoint,
                access_key_id,
                secret_access_key,
                path_style: detected_path_style,
                prefix,
            };

            let response = client
                .post("/api/objectstore", serde_json::to_value(&config)?)
                .await?;
            print_info(&response);

            if !no_test {
                client
                    .post("/api/objectstore/test", serde_json::json!({}))
                    .await?;
                println!("Object store test passed.");
            }
        }
        ObjectStoreCommands::Info => {
            let response = client.get("/api/objectstore").await?;
            print_info(&response);
        }
        ObjectStoreCommands::Test => {
            client
                .post("/api/objectstore/test", serde_json::json!({}))
                .await?;
            println!("Object store test passed.");
        }
        ObjectStoreCommands::Unset => {
            client.delete("/api/objectstore").await?;
            println!("Object store configuration removed.");
        }
        ObjectStoreCommands::Status { app } => {
            let response = client.get(&format!("/api/apps/{app}/objectstore")).await?;
            print_app_link_info(&response);
        }
        ObjectStoreCommands::Link { app, prefix } => {
            let response = client
                .post(
                    &format!("/api/apps/{app}/objectstore"),
                    serde_json::json!({ "prefix": prefix }),
                )
                .await?;
            print_app_link_info(&response);
        }
        ObjectStoreCommands::Unlink { app } => {
            client
                .delete(&format!("/api/apps/{app}/objectstore"))
                .await?;
            println!("Object store credentials removed from '{app}'.");
        }
    }

    Ok(())
}

fn prompt_required(label: &str, current: Option<String>, default: &str) -> Result<String> {
    if let Some(value) = current {
        if value.trim().is_empty() {
            return Err(anyhow!("{label} cannot be empty"));
        }
        return Ok(value);
    }
    let mut input = cliclack::input(label);
    if !default.is_empty() {
        input = input.default_input(default);
    }
    let value: String = input.interact()?;
    if value.trim().is_empty() {
        return Err(anyhow!("{label} cannot be empty"));
    }
    Ok(value)
}

fn prompt_optional(label: &str, current: Option<String>) -> Result<Option<String>> {
    if current.is_some() {
        return Ok(current);
    }
    let value: String = cliclack::input(label).default_input("").interact()?;
    if value.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn print_info(response: &serde_json::Value) {
    if !response["configured"].as_bool().unwrap_or(false) {
        println!("Object store is not configured.");
        return;
    }

    let object_store = &response["object_store"];
    println!(
        "Provider: {}",
        object_store["provider"].as_str().unwrap_or("-")
    );
    println!("Bucket: {}", object_store["bucket"].as_str().unwrap_or("-"));
    println!("Region: {}", object_store["region"].as_str().unwrap_or("-"));
    println!(
        "Endpoint: {}",
        object_store["endpoint"].as_str().unwrap_or("-")
    );
    println!(
        "Access key ID: {}",
        object_store["access_key_id"].as_str().unwrap_or("-")
    );
    println!(
        "Secret access key: {}",
        object_store["secret_access_key"].as_str().unwrap_or("-")
    );
    println!(
        "Path style: {}",
        object_store["path_style"].as_bool().unwrap_or(true)
    );
    println!("Prefix: {}", object_store["prefix"].as_str().unwrap_or("-"));
}

fn print_app_link_info(response: &serde_json::Value) {
    println!("App: {}", response["app"].as_str().unwrap_or("-"));
    println!(
        "Host object store configured: {}",
        response["configured"].as_bool().unwrap_or(false)
    );
    println!("Linked: {}", response["linked"].as_bool().unwrap_or(false));

    let Some(link) = response["link"].as_object() else {
        println!("Link details: unavailable");
        return;
    };

    println!(
        "Provider: {}",
        link.get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("-")
    );
    println!(
        "Bucket: {}",
        link.get("bucket")
            .and_then(|value| value.as_str())
            .unwrap_or("-")
    );
    println!(
        "Region: {}",
        link.get("region")
            .and_then(|value| value.as_str())
            .unwrap_or("-")
    );
    println!(
        "Endpoint: {}",
        link.get("endpoint")
            .and_then(|value| value.as_str())
            .unwrap_or("-")
    );
    println!(
        "Prefix: {}",
        link.get("prefix")
            .and_then(|value| value.as_str())
            .unwrap_or("-")
    );
    println!(
        "Path style: {}",
        link.get("path_style")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    );
    println!(
        "Secret present: {}",
        link.get("secret_present")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    );

    println!("Linked env keys:");
    if let Some(keys) = link.get("linked_keys").and_then(|value| value.as_array()) {
        if keys.is_empty() {
            println!("- none");
        } else {
            for key in keys {
                println!("- {}", key.as_str().unwrap_or("-"));
            }
        }
    }
}
