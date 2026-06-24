pub mod telnet;

use anyhow::Result;

/// Abstraction over both connection paths so install/uninstall ops are path-agnostic.
pub trait DeviceConnection: Send {
    fn run_command(
        &mut self,
        command: &str,
    ) -> impl std::future::Future<Output = Result<String>> + Send;

    fn write_file(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Which physical path to use when reaching the device.
#[derive(Debug, Clone)]
pub enum ConnectionMethod {
    Network {
        admin_ip: String,
        username: String,
        password: String,
    },
    Usb,
}

/// A `DeviceConnection` backed by the telnet nc shell (network path).
pub struct TelnetConnection {
    pub addr: std::net::SocketAddr,
}

impl DeviceConnection for TelnetConnection {
    async fn run_command(&mut self, command: &str) -> Result<String> {
        telnet::send_command_with_output(
            self.addr,
            command,
            false,
            std::time::Duration::from_secs(10),
        )
        .await
    }

    async fn write_file(&mut self, path: &str, content: &[u8]) -> Result<()> {
        telnet::send_file(self.addr, path, content, false).await
    }
}
