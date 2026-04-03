use anyhow::Result;

fn angie_bin() -> String {
    std::env::var("DEKU_ANGIE_BIN").unwrap_or_else(|_| "angie".to_string())
}

fn kill_bin() -> String {
    std::env::var("DEKU_KILL_BIN").unwrap_or_else(|_| "kill".to_string())
}

fn pid_path() -> std::path::PathBuf {
    std::env::var_os("DEKU_ANGIE_PID_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/run/angie/angie.pid"))
}

pub async fn validate() -> Result<()> {
    let output = tokio::process::Command::new(angie_bin())
        .args(["-t", "-q"])
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("angie config validation failed: {}{}", stdout, stderr);
}

/// Reload Angie configuration.
///
/// Sends SIGHUP to the running angie process found in the standard pid file.
/// If the pid file doesn't exist (e.g. Angie not installed yet), this is a
/// no-op with a debug log rather than an error — callers should not fail
/// deploys because Angie isn't installed yet.
pub async fn reload() -> Result<()> {
    let pid_path = pid_path();

    if !pid_path.exists() {
        tracing::debug!("angie pid file not found — skipping reload");
        return Ok(());
    }

    validate().await?;

    let pid_str = tokio::fs::read_to_string(&pid_path).await?;
    let pid = pid_str
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("invalid angie pid: {pid_str}"))?;

    let status = tokio::process::Command::new(kill_bin())
        .args(["-HUP", &pid.to_string()])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("failed to send SIGHUP to angie (pid {pid})");
    }

    tracing::info!(pid, "angie reloaded");
    Ok(())
}
