use anyhow::Result;

/// Reload Angie configuration.
///
/// Sends SIGHUP to the running angie process found in the standard pid file.
/// If the pid file doesn't exist (e.g. Angie not installed yet), this is a
/// no-op with a debug log rather than an error — callers should not fail
/// deploys because Angie isn't installed yet.
pub async fn reload() -> Result<()> {
    let pid_path = std::path::Path::new("/run/angie/angie.pid");

    if !pid_path.exists() {
        tracing::debug!("angie pid file not found — skipping reload");
        return Ok(());
    }

    let pid_str = tokio::fs::read_to_string(pid_path).await?;
    let pid = pid_str
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("invalid angie pid: {pid_str}"))?;

    let status = tokio::process::Command::new("kill")
        .args(["-HUP", &pid.to_string()])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("failed to send SIGHUP to angie (pid {pid})");
    }

    tracing::info!(pid, "angie reloaded");
    Ok(())
}
