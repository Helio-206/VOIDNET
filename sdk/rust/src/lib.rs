pub use void_dns as dns;
pub use void_identity as identity;
pub use void_protocol as protocol;
pub use void_runtime as runtime;

#[derive(Debug, Clone, Default)]
pub struct VoidClientConfig {
    pub bootstrap: Vec<String>,
}

impl VoidClientConfig {
    pub fn with_bootstrap(mut self, address: impl Into<String>) -> Self {
        self.bootstrap.push(address.into());
        self
    }
}
