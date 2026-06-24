pub mod init;

use anyhow::Result;
use serde::Deserialize;

/// Ports reserved by the toolkit itself during install sessions.
/// These will be flagged as conflicts if a manifest declares them.
const RESERVED_PORTS: &[u16] = &[
    24,   // nc shell (ephemeral, opened by exploit)
    8081, // file transfer nc listener (ephemeral, used by send_file)
];

#[derive(Debug, Deserialize)]
pub struct PayloadManifest {
    /// Service name — used as the init script filename and start-stop-daemon identifier
    pub name: String,
    pub version: String,
    /// Absolute path on device where the binary will live
    pub data_dir: String,
    /// Filename of the binary on the device
    pub binary_name: String,
    /// Command-line args passed to the binary in the init script
    #[serde(default)]
    pub args: String,
    pub log_file: String,
    pub pidfile: String,
    /// Ports this service uses at runtime — checked for conflicts at install time
    #[serde(default)]
    pub ports: Vec<u16>,
    /// Optional shell commands to inject before start-stop-daemon in the init script
    #[serde(default)]
    pub pre_start: Vec<String>,
}

impl PayloadManifest {
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Returns any declared ports that overlap with toolkit-reserved ports.
    pub fn port_conflicts(&self) -> Vec<u16> {
        self.ports
            .iter()
            .filter(|p| RESERVED_PORTS.contains(p))
            .copied()
            .collect()
    }

    pub fn binary_path(&self) -> String {
        format!("{}/{}", self.data_dir, self.binary_name)
    }

    pub fn init_script_path(&self) -> String {
        format!("/etc/init.d/{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_conflict_detection() {
        let manifest = PayloadManifest {
            name: "test".into(),
            version: "1.0".into(),
            data_dir: "/data/test".into(),
            binary_name: "test".into(),
            args: String::new(),
            log_file: "/data/test/test.log".into(),
            pidfile: "/tmp/test.pid".into(),
            ports: vec![8081, 9000],
            pre_start: vec![],
        };
        let conflicts = manifest.port_conflicts();
        assert_eq!(conflicts, vec![8081]);
    }
}
