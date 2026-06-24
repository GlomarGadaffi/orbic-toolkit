use anyhow::Result;

use crate::cli::ServiceAction;
use crate::connection::{ConnectionMethod, DeviceConnection};
use crate::orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet};

pub async fn service(
    method: &ConnectionMethod,
    name: &str,
    action: &ServiceAction,
) -> Result<()> {
    let cmd_str = action.as_str();

    match method {
        ConnectionMethod::Network { admin_ip, username, password } => {
            service_network(admin_ip, username, password, name, cmd_str).await
        }
        ConnectionMethod::Usb => service_usb(name, cmd_str).await,
    }
}

pub async fn list(method: &ConnectionMethod) -> Result<()> {
    let output = run_one(method, "ls /etc/init.d/").await?;
    println!("Installed services (/etc/init.d/):");
    for entry in output.split_whitespace() {
        println!("  {entry}");
    }
    Ok(())
}

pub async fn status(method: &ConnectionMethod, name: &str) -> Result<()> {
    let init_path = format!("/etc/init.d/{name}");
    let output = run_one(method, &format!("{init_path} status")).await?;
    println!("{}", output.trim());
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

async fn run_one(method: &ConnectionMethod, command: &str) -> Result<String> {
    match method {
        ConnectionMethod::Network { admin_ip, username, password } => {
            use crate::connection::telnet::send_command_with_output;
            login_and_exploit(admin_ip, username, password).await?;
            wait_for_telnet(admin_ip).await?;
            let addr = telnet_addr(admin_ip)?;
            send_command_with_output(addr, command, false, std::time::Duration::from_secs(10))
                .await
        }
        ConnectionMethod::Usb => {
            use crate::orbic::usb::open_connection;
            let mut conn = open_connection(None).await?;
            conn.run_command(command).await
        }
    }
}

async fn service_network(
    admin_ip: &str,
    username: &str,
    password: &str,
    name: &str,
    action: &str,
) -> Result<()> {
    use crate::connection::telnet::send_command;

    login_and_exploit(admin_ip, username, password).await?;
    wait_for_telnet(admin_ip).await?;
    let addr = telnet_addr(admin_ip)?;
    let init_path = format!("/etc/init.d/{name}");
    send_command(addr, &format!("{init_path} {action}"), "", false).await?;
    Ok(())
}

async fn service_usb(name: &str, action: &str) -> Result<()> {
    use crate::orbic::usb::open_connection;
    let mut conn = open_connection(None).await?;
    let init_path = format!("/etc/init.d/{name}");
    conn.run_command(&format!("{init_path} {action}")).await?;
    Ok(())
}
