use deku_core::types::{App, Deployment};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub app: App,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildContext {
    pub app: App,
    pub source_dir: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DeployContext {
    pub app: App,
    pub deployment: Deployment,
    pub data_dir: PathBuf,
}
