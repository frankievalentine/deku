use crate::error::Result;
use crate::types::App;
use std::path::Path;

pub trait DekuPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn enabled(&self) -> bool {
        true
    }
}
