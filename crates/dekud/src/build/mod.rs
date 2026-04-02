// Builder trait and implementations.
// Full implementation in Milestone 3.

use async_trait::async_trait;
use deku_core::error::Result;
use deku_plugin_sdk::context::BuildContext;
use std::path::Path;

pub struct BuiltImage {
    pub image_id: String,
    pub tag: String,
    pub exposed_ports: Vec<u16>,
}

#[async_trait]
pub trait Builder: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, source: &Path) -> bool;
    async fn build(&self, ctx: &BuildContext) -> Result<BuiltImage>;
}
