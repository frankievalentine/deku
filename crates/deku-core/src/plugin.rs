pub trait DekuPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn enabled(&self) -> bool {
        true
    }
}
