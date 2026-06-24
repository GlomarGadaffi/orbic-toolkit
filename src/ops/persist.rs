use std::net::SocketAddr;

use anyhow::Result;

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

pub async fn persist_nc_shell(addr: SocketAddr) -> Result<()> {
    use crate::connection::telnet::{send_command, send_file};

    send_command(addr, "mount -o remount,rw /dev/ubi0_0 /", "", false).await?;
    send_file(addr, INIT_PATH, INIT_SCRIPT, false).await?;
    send_command(addr, &format!("chmod 755 {INIT_PATH}"), "", false).await?;
    println!("Persistent nc shell installed ({INIT_PATH}).");
    println!("Management: /etc/init.d/S99orbic-shell {{start|stop|restart}}");
    Ok(())
}

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
