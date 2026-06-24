use anyhow::Result;

use crate::connection::{ConnectionMethod, DeviceConnection};
use crate::orbic::exploit::{login_and_exploit, telnet_addr, wait_for_telnet};
use crate::payload::{PayloadManifest, init};

pub async fn install(
    method: &ConnectionMethod,
    manifest: &PayloadManifest,
    binary: &[u8],
    no_init: bool,
    no_reboot: bool,
) -> Result<()> {
    let conflicts = manifest.port_conflicts();
    if !conflicts.is_empty() {
        eprintln!(
            "Warning: manifest declares ports {:?} which overlap with toolkit-reserved ports \
             (24 = exploit shell, 8081 = file-transfer listener). \
             Port 8081 is only in use during file transfer (ephemeral) — ensure no persistent \
             runtime conflict.",
            conflicts
        );
    }

    match method {
        ConnectionMethod::Network { admin_ip, username, password } => {
            install_network(admin_ip, username, password, manifest, binary, no_init, no_reboot)
                .await
        }
        ConnectionMethod::Usb => install_usb(manifest, binary, no_init, no_reboot).await,
    }
}

async fn install_network(
    admin_ip: &str,
    username: &str,
    password: &str,
    manifest: &PayloadManifest,
    binary: &[u8],
    no_init: bool,
    no_reboot: bool,
) -> Result<()> {
    use crate::connection::telnet::{send_command, send_file};

    print!("Logging in and starting telnet... ");
    login_and_exploit(admin_ip, username, password).await?;
    println!("done");

    print!("Waiting for telnet on port 24... ");
    wait_for_telnet(admin_ip).await?;
    println!("done");

    let addr = telnet_addr(admin_ip)?;

    // Remount root rw (required on Orbic and Moxee)
    send_command(addr, "mount -o remount,rw /dev/ubi0_0 /", "exit code 0", false).await?;

    // Create data directory
    send_command(
        addr,
        &format!("mkdir -p {}", manifest.data_dir),
        "exit code 0",
        false,
    )
    .await?;

    // Push binary
    println!("Pushing binary to {}...", manifest.binary_path());
    send_file(addr, &manifest.binary_path(), binary, false).await?;
    send_command(
        addr,
        &format!("chmod +x {}", manifest.binary_path()),
        "exit code 0",
        false,
    )
    .await?;

    if !no_init {
        let init_script = init::render(manifest);
        println!("Installing init script at {}...", manifest.init_script_path());
        send_file(addr, &manifest.init_script_path(), init_script.as_bytes(), false).await?;
        send_command(
            addr,
            &format!("chmod 755 {}", manifest.init_script_path()),
            "exit code 0",
            false,
        )
        .await?;
    }

    finish(addr, no_reboot, admin_ip).await
}

async fn install_usb(
    manifest: &PayloadManifest,
    binary: &[u8],
    no_init: bool,
    no_reboot: bool,
) -> Result<()> {
    use crate::orbic::usb::open_connection;

    let mut conn = open_connection(None).await?;

    // Remount root rw
    conn.run_command("mount -o remount,rw /dev/ubi0_0 /").await?;

    // Create data directory
    conn.run_command(&format!("mkdir -p {}", manifest.data_dir)).await?;

    // Push binary
    println!("Pushing binary to {}...", manifest.binary_path());
    conn.write_file(&manifest.binary_path(), binary).await?;
    conn.run_command(&format!("chmod +x {}", manifest.binary_path())).await?;

    if !no_init {
        let init_script = init::render(manifest);
        println!("Installing init script at {}...", manifest.init_script_path());
        conn.write_file(&manifest.init_script_path(), init_script.as_bytes()).await?;
        conn.run_command(&format!("chmod 755 {}", manifest.init_script_path())).await?;
    }

    if !no_reboot {
        println!("Installation complete. Rebooting...");
        conn.run_command("shutdown -r -t 1 now").await.ok();
        println!("Device rebooting.");
    } else {
        println!("Installation complete (no reboot).");
    }

    Ok(())
}

async fn finish(
    addr: std::net::SocketAddr,
    no_reboot: bool,
    admin_ip: &str,
) -> Result<()> {
    use crate::connection::telnet::send_command;

    if !no_reboot {
        println!("Installation complete. Rebooting...");
        send_command(addr, "shutdown -r -t 1 now", "", false).await.ok();
        println!(
            "Device rebooting. Service will start automatically after boot (http://{admin_ip})."
        );
    } else {
        println!("Installation complete (no reboot requested).");
    }
    Ok(())
}
