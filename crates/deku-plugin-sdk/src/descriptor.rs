use crate::hooks::{
    AppCreateHook, AppDestroyHook, PostBuildHook, PostDeployHook, PreBuildHook, PreDeployHook,
};

pub trait PluginDescriptor: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn pre_build(&self) -> Option<Box<dyn PreBuildHook>> {
        None
    }
    fn post_build(&self) -> Option<Box<dyn PostBuildHook>> {
        None
    }
    fn pre_deploy(&self) -> Option<Box<dyn PreDeployHook>> {
        None
    }
    fn post_deploy(&self) -> Option<Box<dyn PostDeployHook>> {
        None
    }
    fn app_create(&self) -> Option<Box<dyn AppCreateHook>> {
        None
    }
    fn app_destroy(&self) -> Option<Box<dyn AppDestroyHook>> {
        None
    }
}
