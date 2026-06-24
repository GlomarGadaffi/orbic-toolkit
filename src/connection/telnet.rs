use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

#[cfg(unix)]
use std::os::fd::AsRawFd;

// Unique markers so we can extract output without accidentally matching the
// echoed command text. The inner quotes prevent matching the echoed COMMAND_DONE.
const CMD_START: &str = "ORBIC_TOOLKIT_CMD_START";
const CMD_DONE: &str = "ORBIC_TOOLKIT_CMD_DONE";

/// Run a command over the nc shell and return its stdout as a String.
pub async fn send_command_with_output(
    addr: SocketAddr,
    command: &str,
    wait_for_prompt: bool,
    command_timeout: Duration,
) -> Result<String> {
    if command.contains('\n') {
        bail!("multi-line commands are not allowed");
    }

    let stream = TcpStream::connect(addr).await?;
    let (mut reader, mut writer) = stream.into_split();

    if wait_for_prompt {
        while reader.read_u8().await? != b'#' {}
    }

    // Echo markers with inner quotes so the echoed command text doesn't match CMD_DONE.
    writer
        .write_all(
            format!(
                "echo ORBIC_TOOLKIT_'CMD'_START; {command}; echo ORBIC_TOOLKIT_'CMD'_DONE\r\n"
            )
            .as_bytes(),
        )
        .await?;

    let mut read_buf = Vec::new();
    timeout(command_timeout, async {
        while let Ok(byte) = reader.read_u8().await {
            read_buf.push(byte);
            if byte == b'\n' {
                let response = String::from_utf8_lossy(&read_buf);
                if response.contains(CMD_DONE) {
                    break;
                }
            }
        }
    })
    .await
    .with_context(|| format!("command timed out after {}s", command_timeout.as_secs()))?;

    let s = String::from_utf8_lossy(&read_buf);
    let start = s.rfind(CMD_START);
    let end = s.rfind(CMD_DONE);
    match (start, end) {
        (Some(start), Some(end)) => {
            let start = start + CMD_START.len();
            Ok(s[start..end].trim_start_matches(['\r', '\n']).to_string())
        }
        _ => bail!("failed to parse command output: {s:?}"),
    }
}

/// Run a command and verify `expected_output` appears in the result.
pub async fn send_command(
    addr: SocketAddr,
    command: &str,
    expected_output: &str,
    wait_for_prompt: bool,
) -> Result<()> {
    let wrapped = format!("{command}; echo command done, exit code $?");
    let output =
        send_command_with_output(addr, &wrapped, wait_for_prompt, Duration::from_secs(10)).await?;
    if !expected_output.is_empty() && !output.contains(expected_output) {
        bail!("{expected_output:?} not found in output: {output}");
    }
    Ok(())
}

/// Transfer a file to the device via a temporary nc listener on port 8081.
/// Verifies integrity with MD5 before committing the file to its final path.
pub async fn send_file(
    addr: SocketAddr,
    filename: &str,
    payload: &[u8],
    wait_for_prompt: bool,
) -> Result<()> {
    print!("Sending {filename} ... ");

    // Allow 30 s base + 2 s per MB for slow WiFi links
    let transfer_timeout =
        Duration::from_secs(30 + (payload.len() as u64 / (1024 * 1024)).max(1) * 2);

    let filename_owned = filename.to_owned();
    let nc_handle = tokio::spawn(async move {
        send_command_with_output(
            addr,
            &format!("nc -l -p 8081 2>&1 >{filename_owned}.tmp"),
            wait_for_prompt,
            transfer_timeout,
        )
        .await
    });

    let mut recv_addr = addr;
    recv_addr.set_port(8081);

    let mut attempts = 0u32;
    let stream = loop {
        // Exponential back-off while nc starts up
        sleep(Duration::from_millis(100 * (1 << attempts))).await;
        match TcpStream::connect(recv_addr).await {
            Ok(s) => break Ok(s),
            Err(e) => {
                attempts += 1;
                if attempts > 3 {
                    break Err(e);
                }
                print!("attempt {attempts}... ");
            }
        }
    };

    let send_result: Result<()> = async {
        let mut stream = stream?;
        stream.write_all(payload).await?;
        // Give the Orbic time to flush nc's application buffer to disk before we close
        sleep(Duration::from_millis(1000)).await;
        Ok(())
    }
    .await;

    let nc_output = nc_handle
        .await
        .context("background nc task failed")??;

    if let Err(e) = send_result {
        bail!("Failed to send data: {e}. nc output: '{}'", nc_output.trim());
    }

    let checksum = md5::compute(payload);
    send_command(
        addr,
        &format!("md5sum {filename}.tmp"),
        &format!("{checksum:x}  {filename}.tmp"),
        wait_for_prompt,
    )
    .await
    .with_context(|| {
        format!(
            "File transfer failed (checksum mismatch). nc output: '{}'. Expected: {:x}",
            nc_output.trim(),
            checksum
        )
    })?;

    send_command(
        addr,
        &format!("mv {filename}.tmp {filename}"),
        "exit code 0",
        wait_for_prompt,
    )
    .await?;

    println!("ok");
    Ok(())
}

/// Receive a file from the device by cat-ing it and extracting between markers.
pub async fn recv_file(addr: SocketAddr, remote_path: &str) -> Result<Vec<u8>> {
    let output = send_command_with_output(
        addr,
        &format!("cat {remote_path} | base64"),
        false,
        Duration::from_secs(30),
    )
    .await?;
    let decoded = base64_light::base64_decode(output.trim());
    Ok(decoded)
}

/// Open a bidirectional interactive shell over TCP (the nc backdoor on port 24).
pub async fn interactive_shell(admin_ip: &str, shell_port: u16) -> Result<()> {
    use std::str::FromStr;

    let shell_addr = std::net::SocketAddr::from_str(&format!("{admin_ip}:{shell_port}"))?;
    let mut stream = TcpStream::connect(shell_addr)
        .await
        .context("Failed to connect to shell")?;

    let stdin = tokio::io::stdin();

    #[cfg(unix)]
    let _raw_guard = RawTerminal::new(stdin.as_raw_fd()).ok();

    let mut stdio = tokio::io::join(stdin, tokio::io::stdout());
    let _ = tokio::io::copy_bidirectional(&mut stream, &mut stdio).await;

    println!();
    // tokio stdin blocks forever after the remote closes; exit directly
    std::process::exit(0)
}

#[cfg(unix)]
struct RawTerminal {
    fd: std::os::fd::RawFd,
    original: termios::Termios,
}

#[cfg(unix)]
impl RawTerminal {
    fn new(fd: std::os::fd::RawFd) -> Result<Self> {
        let original = termios::Termios::from_fd(fd)?;
        let mut t = original;
        termios::cfmakeraw(&mut t);
        termios::tcsetattr(fd, termios::TCSANOW, &t)?;
        Ok(RawTerminal { fd, original })
    }
}

#[cfg(unix)]
impl Drop for RawTerminal {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(self.fd, termios::TCSANOW, &self.original);
    }
}
