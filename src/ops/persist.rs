use std::net::SocketAddr;

use anyhow::Result;

use crate::connection::{DeviceConnection, TelnetConnection};

pub const INIT_PATH: &str = "/etc/init.d/S99orbic-shell";

// Busybox nc -ll keeps the port open across multiple connections (persistent listener).
// The init script restarts it on every boot via the S99 ordering.
const INIT_SCRIPT: &[u8] = b"#!/bin/sh\n\
case \"$1\" in\n\
  start)   busybox nc -ll -p 24 -e /bin/sh &;;\n\
  stop)    kill $(busybox pidof nc) 2>/dev/null; true;;\n\
  restart) $0 stop; $0 start;;\n\
  *)       echo \"Usage: $0 {start|stop|restart}\"; exit 1;;\nesac\n";

pub async fn prompt() -> bool {
    use std::io::Write;
    use tokio::io::{AsyncBufReadExt, BufReader};

    print!("Make nc shell persistent across reboots? [y/N] ");
    std::io::stdout().flush().ok();

    let mut line = String::new();
    let mut reader = BufReader::new(tokio::io::stdin());
    reader.read_line(&mut line).await.ok();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

pub async fn persist_nc_shell(conn: &mut TelnetConnection) -> Result<()> {
    conn.run_command("mount -o remount,rw /dev/ubi0_0 /").await?;
    conn.write_file(INIT_PATH, INIT_SCRIPT).await?;
    conn.run_command(&format!("chmod 755 {INIT_PATH}")).await?;
    println!("Persistent nc shell installed ({INIT_PATH}).");
    println!("Management: /etc/init.d/S99orbic-shell {{start|stop|restart}}");
    Ok(())
}

/// Not wired to a CLI command yet. The nc-shell persistence this undoes is the
/// unauthenticated-root-on-boot mechanism the README (v0.2 roadmap) already
/// flags for replacement by key-authenticated Dropbear SSH (#7/#8) -- adding
/// new CLI surface around it now would expand a feature the project wants to
/// retire, not fix. Kept for symmetry with `persist_nc_shell` and for anyone
/// removing it manually via `orbic-toolkit shell` in the meantime.
#[allow(dead_code)]
pub async fn remove_persist(addr: SocketAddr) -> Result<()> {
    use crate::connection::telnet::send_command;

    send_command(addr, "mount -o remount,rw /dev/ubi0_0 /", "", false).await?;
    send_command(
        addr,
        &format!("/etc/init.d/S99orbic-shell stop; rm {INIT_PATH}"),
        "",
        false,
    )
    .await?;
    println!("Persistent nc shell removed.");
    Ok(())
}
