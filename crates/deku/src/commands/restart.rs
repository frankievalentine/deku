use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Args;

#[derive(Debug, Clone, Args, Default)]
pub struct RestartArgs;

const SERVICE_NAME: &str = "deku";

trait ServiceManager {
    fn restart(&self, service: &str) -> Result<()>;
}

struct SystemctlManager;

impl ServiceManager for SystemctlManager {
    fn restart(&self, service: &str) -> Result<()> {
        let output = Command::new("systemctl")
            .args(["restart", service])
            .output()
            .with_context(|| {
                format!(
                    "`deku restart` requires a systemd-managed Linux host with `systemctl` available"
                )
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "Run this command as root on a systemd-managed Deku host.".to_string()
        };

        bail!(
            "`systemctl restart {service}` failed. {detail}",
            service = service,
            detail = detail
        );
    }
}

pub fn run(_: RestartArgs) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("`deku restart` is supported only on Linux hosts that use systemd.");
    }

    restart_with(&SystemctlManager)
}

fn restart_with(manager: &dyn ServiceManager) -> Result<()> {
    manager.restart(SERVICE_NAME)?;
    println!("Restarted the `{SERVICE_NAME}` service.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::{bail, Result};

    use super::{restart_with, ServiceManager, SERVICE_NAME};

    struct MockServiceManager {
        result: Result<()>,
    }

    impl ServiceManager for MockServiceManager {
        fn restart(&self, service: &str) -> Result<()> {
            assert_eq!(service, SERVICE_NAME);
            match &self.result {
                Ok(()) => Ok(()),
                Err(error) => bail!(error.to_string()),
            }
        }
    }

    #[test]
    fn restart_uses_dedu_service_name() {
        let manager = MockServiceManager { result: Ok(()) };
        assert!(restart_with(&manager).is_ok());
    }

    #[test]
    fn restart_surfaces_failures() {
        let manager = MockServiceManager {
            result: Err(anyhow::anyhow!("permission denied")),
        };

        let error = restart_with(&manager).expect_err("restart should fail");
        assert!(error.to_string().contains("permission denied"));
    }
}
