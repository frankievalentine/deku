use anyhow::Result;
use clap::Args;
use deku_core::version::release_version;

#[derive(Debug, Clone, Args, Default)]
pub struct VersionArgs;

pub fn run(_: VersionArgs) -> Result<()> {
    println!("{}", release_version());
    Ok(())
}
