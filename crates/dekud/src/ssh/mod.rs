// Embedded SSH server for git push and remote CLI commands.
// Full implementation in Milestone 1.3.

use crate::api::SharedState;
use anyhow::Result;

pub async fn serve(_state: SharedState) -> Result<()> {
    tracing::info!("SSH server placeholder (not yet implemented)");
    // Keep the future pending so tokio::try_join! doesn't complete
    std::future::pending::<()>().await;
    Ok(())
}
