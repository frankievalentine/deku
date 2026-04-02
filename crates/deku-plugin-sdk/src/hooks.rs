use crate::context::{AppContext, BuildContext, DeployContext};
use async_trait::async_trait;
use deku_core::error::Result;

#[async_trait]
pub trait PreBuildHook: Send + Sync {
    async fn pre_build(&self, ctx: &BuildContext) -> Result<()>;
}

#[async_trait]
pub trait PostBuildHook: Send + Sync {
    async fn post_build(&self, ctx: &BuildContext) -> Result<()>;
}

#[async_trait]
pub trait PreDeployHook: Send + Sync {
    async fn pre_deploy(&self, ctx: &DeployContext) -> Result<()>;
}

#[async_trait]
pub trait PostDeployHook: Send + Sync {
    async fn post_deploy(&self, ctx: &DeployContext) -> Result<()>;
}

#[async_trait]
pub trait AppCreateHook: Send + Sync {
    async fn app_create(&self, ctx: &AppContext) -> Result<()>;
}

#[async_trait]
pub trait AppDestroyHook: Send + Sync {
    async fn app_destroy(&self, ctx: &AppContext) -> Result<()>;
}
