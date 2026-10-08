use anyhow::Result;

use crate::connection::{ConnectionMethod, DeviceConnection, TelnetConnection};
use crate::orbic::remote_access::{login_and_enable_remote_shell, telnet_addr, wait_for_telnet};

pub async fn uninstall(
    method: &ConnectionMethod,
    name: &str,
    no_reboot: bool,
) -> Result<()> {
    match method {
        ConnectionMethod::Network { admin_ip, username, password } => {
            uninstall_network(admin_ip, username, password, name, no_reboot).await
        }
        ConnectionMethod::Usb => uninstall_usb(name, no_reboot).await,
    }
}

async fn uninstall_network(
    admin_ip: &str,
    username: &str,
    password: &str,
    name: &str,
    no_reboot: bool,
) -> Result<()> {
    print!("Logging in and starting telnet... ");
    login_and_enable_remote_shell(admin_ip, username, password).await?;
    println!("done");

    print!("Waiting for telnet... ");
    wait_for_telnet(admin_ip).await?;
    println!("done");

    let mut conn = TelnetConnection { addr: telnet_addr(admin_ip)? };
    let init_path = format!("/etc/init.d/{name}");

    // Stop service (ignore errors — it may not be running)
    conn.run_command(&format!("{init_path} stop")).await.ok();

    // Remove init script
    println!("Removing init script {init_path}...");
    conn.run_command_checked(&format!("rm -f {init_path}"), "exit code 0").await?;

    // Remove data directory
    let data_dir = format!("/data/{name}");
    println!("Removing data directory {data_dir}...");
    conn.run_command_checked(&format!("rm -rf {data_dir}"), "exit code 0").await?;

    if !no_reboot {
        println!("Uninstall complete. Rebooting...");
        conn.run_command("shutdown -r -t 1 now").await.ok();
    } else {
        println!("Uninstall complete (no reboot).");
    }
    Ok(())
}

async fn uninstall_usb(name: &str, no_reboot: bool) -> Result<()> {
    use crate::orbic::usb::open_connection;

    let mut conn = open_connection(None).await?;
    let init_path = format!("/etc/init.d/{name}");
    let data_dir = format!("/data/{name}");

    conn.run_command(&format!("{init_path} stop")).await.ok();
    conn.run_command(&format!("rm -f {init_path}")).await?;
    conn.run_command(&format!("rm -rf {data_dir}")).await?;

    if !no_reboot {
        println!("Uninstall complete. Rebooting...");
        conn.run_command("shutdown -r -t 1 now").await.ok();
    } else {
        println!("Uninstall complete (no reboot).");
    }
    Ok(())
}
