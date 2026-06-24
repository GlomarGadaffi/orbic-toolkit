mod cli;
mod connection;
mod ops;
mod orbic;
mod payload;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, Via};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Shell { rootshell } => match cli.via {
            Via::Network => {
                let method = cli.connection_method()?;
                let ConnectionMethod::Network { ref admin_ip, ref username, ref password } =
                    method
                else {
                    unreachable!()
                };
                use orbic::exploit::{login_and_exploit, wait_for_telnet};
                use orbic::exploit::TELNET_PORT;
                print!("Logging in and starting telnet... ");
                login_and_exploit(admin_ip, username, password).await?;
                println!("done");
                print!("Waiting for shell on port {TELNET_PORT}... ");
                wait_for_telnet(admin_ip).await?;
                println!("done");
                eprintln!(
                    "Shell ready. This is a limited nc-based shell — prompt may not be visible."
                );
                connection::telnet::interactive_shell(admin_ip, TELNET_PORT).await?;
            }
            Via::Usb => {
                let rootshell_bytes = rootshell
                    .as_deref()
                    .map(std::fs::read)
                    .transpose()?;
                orbic::usb::interactive_shell(rootshell_bytes.as_deref()).await?;
            }
        },

        Command::StartTelnet => {
            if !matches!(cli.via, Via::Network) {
                anyhow::bail!("start-telnet is only available for --via network");
            }
            let method = cli.connection_method()?;
            let ConnectionMethod::Network { ref admin_ip, ref username, ref password } = method
            else {
                unreachable!()
            };
            use orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet, TELNET_PORT};
            print!("Logging in and starting telnet... ");
            login_and_exploit(admin_ip, username, password).await?;
            println!("done");
            print!("Waiting for shell on port {TELNET_PORT}... ");
            wait_for_telnet(admin_ip).await?;
            println!("done");
            println!("Telnet backdoor ready on {admin_ip}:{TELNET_PORT}");
            if ops::persist::prompt().await {
                let addr = telnet_addr(admin_ip)?;
                ops::persist::persist_nc_shell(addr).await?;
            }
        }

        Command::Run { command } => {
            let output = match cli.via {
                Via::Network => {
                    let method = cli.connection_method()?;
                    let ConnectionMethod::Network { ref admin_ip, ref username, ref password } =
                        method
                    else {
                        unreachable!()
                    };
                    use connection::telnet::send_command_with_output;
                    use orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet};
                    login_and_exploit(admin_ip, username, password).await?;
                    wait_for_telnet(admin_ip).await?;
                    let addr = telnet_addr(admin_ip)?;
                    send_command_with_output(addr, command, false, std::time::Duration::from_secs(30))
                        .await?
                }
                Via::Usb => {
                    use orbic::usb::open_connection;
                    let mut conn = open_connection(None).await?;
                    conn.run_command(command).await?
                }
            };
            print!("{output}");
        }

        Command::Push { local, remote } => match cli.via {
            Via::Network => {
                let method = cli.connection_method()?;
                let ConnectionMethod::Network { ref admin_ip, ref username, ref password } = method
                else {
                    unreachable!()
                };
                use connection::telnet::send_file;
                use orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet};
                let content = std::fs::read(local)?;
                login_and_exploit(admin_ip, username, password).await?;
                wait_for_telnet(admin_ip).await?;
                let addr = telnet_addr(admin_ip)?;
                send_file(addr, remote, &content, false).await?;
            }
            Via::Usb => {
                use orbic::usb::open_connection;
                let content = std::fs::read(local)?;
                let mut conn = open_connection(None).await?;
                conn.write_file(remote, &content).await?;
                println!("Pushed {local} → {remote}");
            }
        },

        Command::Pull { remote, local } => {
            let method = cli.connection_method()?;
            let ConnectionMethod::Network { ref admin_ip, ref username, ref password } = method
            else {
                anyhow::bail!("pull is currently only supported via --via network");
            };
            use connection::telnet::recv_file;
            use orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet};
            login_and_exploit(admin_ip, username, password).await?;
            wait_for_telnet(admin_ip).await?;
            let addr = telnet_addr(admin_ip)?;
            let data = recv_file(addr, remote).await?;
            std::fs::write(local, &data)?;
            println!("Pulled {remote} → {local} ({} bytes)", data.len());
        }

        Command::Install { manifest, binary, no_init, no_reboot } => {
            let method = cli.connection_method()?;
            let manifest = payload::PayloadManifest::from_file(manifest)?;
            let binary = std::fs::read(binary)?;
            ops::install::install(&method, &manifest, &binary, *no_init, *no_reboot).await?;
        }

        Command::Uninstall { name, no_reboot } => {
            let method = cli.connection_method()?;
            ops::uninstall::uninstall(&method, name, *no_reboot).await?;
        }

        Command::List => {
            let method = cli.connection_method()?;
            ops::service::list(&method).await?;
        }

        Command::Status { name } => {
            let method = cli.connection_method()?;
            ops::service::status(&method, name).await?;
        }

        Command::Service { name, action } => {
            let method = cli.connection_method()?;
            ops::service::service(&method, name, action).await?;
        }
    }

    Ok(())
}

use connection::{ConnectionMethod, DeviceConnection};
