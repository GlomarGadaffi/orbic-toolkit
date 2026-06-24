use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Via {
    Network,
    Usb,
}

/// Action for the `service` subcommand.
#[derive(Debug, Clone)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Status,
}

impl ServiceAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceAction::Start => "start",
            ServiceAction::Stop => "stop",
            ServiceAction::Restart => "restart",
            ServiceAction::Status => "status",
        }
    }
}

impl std::str::FromStr for ServiceAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "start" => Ok(ServiceAction::Start),
            "stop" => Ok(ServiceAction::Stop),
            "restart" => Ok(ServiceAction::Restart),
            "status" => Ok(ServiceAction::Status),
            _ => Err(format!("unknown action '{s}'; use: start, stop, restart, status")),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "orbic-toolkit",
    about = "Universal Orbic RC400L root-access & payload installer",
    long_about = "Provides full root shell access and generic payload management \
                  (install / uninstall / service control) for the Orbic RC400L \
                  mobile hotspot, using the SetRemoteAccessCfg exploit chain \
                  pioneered by the EFF rayhunter project."
)]
pub struct Cli {
    /// Device admin IP address (network path only)
    #[arg(long, default_value = "192.168.1.1", global = true)]
    pub admin_ip: String,

    /// Admin username (network path only)
    #[arg(long, default_value = "admin", global = true)]
    pub username: String,

    /// Admin password — required when --via network
    #[arg(long, global = true)]
    pub password: Option<String>,

    /// Connection method: network (WiFi + HTTP exploit) or usb (ADB)
    #[arg(long, value_enum, default_value = "network", global = true)]
    pub via: Via,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn require_password(&self) -> anyhow::Result<&str> {
        self.password
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--password is required when using --via network"))
    }

    pub fn connection_method(&self) -> anyhow::Result<crate::connection::ConnectionMethod> {
        match self.via {
            Via::Network => Ok(crate::connection::ConnectionMethod::Network {
                admin_ip: self.admin_ip.clone(),
                username: self.username.clone(),
                password: self.require_password()?.to_owned(),
            }),
            Via::Usb => Ok(crate::connection::ConnectionMethod::Usb),
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Drop into an interactive root shell
    ///
    /// Network path: opens the nc backdoor on port 24 and proxies stdin/stdout.
    /// USB path: opens an ADB shell (root if rootshell is installed).
    Shell {
        /// (USB only) Path to a pre-compiled ARM rootshell binary to install for full root
        #[arg(long)]
        rootshell: Option<String>,
    },

    /// (Network only) Open the nc backdoor on port 24 without entering a shell
    StartTelnet,

    /// Run a single command on the device and print its output
    Run {
        command: String,
    },

    /// Push a local file to the device (integrity-verified)
    Push {
        /// Local path
        local: String,
        /// Remote path on device
        remote: String,
    },

    /// Pull a file from the device to a local path
    Pull {
        /// Remote path on device
        remote: String,
        /// Local destination path
        local: String,
    },

    /// Install a payload (binary + optional init script) described by a TOML manifest
    Install {
        /// Path to payload.toml manifest file
        manifest: String,

        /// Path to the ARM binary to push to the device
        #[arg(long)]
        binary: String,

        /// Push the binary only — skip writing the init script
        #[arg(long)]
        no_init: bool,

        /// Skip rebooting after installation (useful when chaining multiple installs)
        #[arg(long)]
        no_reboot: bool,
    },

    /// Uninstall a payload by name (stops service, removes binary + init script)
    Uninstall {
        /// Name of the payload (as declared in its manifest)
        name: String,

        /// Skip rebooting after removal
        #[arg(long)]
        no_reboot: bool,
    },

    /// List payloads installed in /etc/init.d/
    List,

    /// Check whether a named service is currently running
    Status {
        name: String,
    },

    /// Send a start / stop / restart / status command to an installed service
    Service {
        name: String,
        action: ServiceAction,
    },
}
