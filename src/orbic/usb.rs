//! USB path: switches the Orbic into ADB mode via a USB vendor control request,
//! then communicates over ADB + AT+SYSCMD for privileged operations.
//! Ported from rayhunter/installer/src/orbic.rs.

use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use adb_client::{ADBDeviceExt, ADBUSBDevice, RustADBError};
use nusb::Interface;
use nusb::transfer::{Control, ControlType, Recipient, RequestBuffer};
use sha2::{Digest, Sha256};
use tokio::time::sleep;

use crate::connection::DeviceConnection;

const VENDOR_ID: u16 = 0x05c6;
const PRODUCT_ID: u16 = 0xf601; // ADB mode
const PRODUCT_ID_RNDIS: u16 = 0xf626; // RNDIS mode (pre-switch)
const PRODUCT_ID_COMBO: u16 = 0xf622; // RNDIS + ADB combo mode

const INTERFACE: u8 = 1;

#[cfg(target_os = "windows")]
const RNDIS_INTERFACE: u8 = 0;
#[cfg(not(target_os = "windows"))]
const RNDIS_INTERFACE: u8 = 1;

// ── Error messages ──────────────────────────────────────────────────────────

const NOT_FOUND: &str = "No Orbic device found.\n\
    Make sure it is plugged in and turned on.\n\
    If it is plugged in, run `lsusb` and file a bug with the output.";

const BUSY: &str = "The Orbic is plugged in but in use by another program.\n\
    Close any program using USB devices; if adb is installed, kill the adb daemon.";

#[cfg(any(target_os = "macos", target_os = "windows"))]
const BUSY_MAC_WIN: &str = "Permission denied.\n\
    On macOS/Windows this is usually caused by another program (e.g. adb daemon) using the device.";

#[cfg(target_os = "windows")]
const WINDOWS_WARNING: &str = "\
    *** WARNING: USB INSTALL ON WINDOWS IS NOT FULLY SUPPORTED ***\n\
    *** THIS MAY BRICK YOUR DEVICE                              ***\n\
    *** USE macOS OR LINUX IF POSSIBLE                          ***";

// ── Public API ───────────────────────────────────────────────────────────────

/// A `DeviceConnection` backed by ADB over USB.
/// All privileged commands run via `/bin/rootshell -c "..."` if rootshell is
/// installed, otherwise via direct AT+SYSCMD (no guarantee of root level).
pub struct AdbConnection {
    device: ADBUSBDevice,
    has_rootshell: bool,
}

impl DeviceConnection for AdbConnection {
    async fn run_command(&mut self, command: &str) -> Result<String> {
        if self.has_rootshell {
            adb_command(
                &mut self.device,
                &["/bin/rootshell", "-c", &format!("\"{command}\"")],
            )
        } else {
            adb_command(&mut self.device, &["sh", "-c", command])
        }
    }

    async fn write_file(&mut self, path: &str, content: &[u8]) -> Result<()> {
        install_file(&mut self.device, path, content).await
    }
}

/// Switch the device into ADB mode (if not already) and return an `AdbConnection`.
///
/// Optionally installs `/bin/rootshell` (setuid 4755) for full root access.
/// Pass `rootshell_binary` as `None` to skip that step.
pub async fn open_connection(rootshell_binary: Option<&[u8]>) -> Result<AdbConnection> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{WINDOWS_WARNING}");
        eprint!("Enter 'yes' to continue: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "yes" {
            bail!("USB install aborted by user.");
        }
    }

    eprintln!("Switching device into ADB mode...");
    enable_command_mode()?;

    eprintln!("Waiting for ADB...");
    let mut device = get_adb().await?;

    let has_rootshell = if let Some(binary) = rootshell_binary {
        eprintln!("Installing rootshell...");
        setup_rootshell(&mut device, binary).await?;
        eprintln!("Rootshell installed — all commands will run as root.");
        true
    } else {
        eprintln!("No rootshell binary provided — proceeding with direct ADB shell.");
        false
    };

    Ok(AdbConnection { device, has_rootshell })
}

/// Open an interactive ADB shell on the device.
pub async fn interactive_shell(rootshell_binary: Option<&[u8]>) -> Result<()> {
    let mut conn = open_connection(rootshell_binary).await?;
    eprintln!("Entering ADB shell. The prompt may not be visible.");
    conn.device
        .shell(&mut std::io::stdin(), Box::new(std::io::stdout()))?;
    Ok(())
}

// ── Device-mode switching ────────────────────────────────────────────────────

/// Send the USB vendor control request that switches the device from RNDIS to
/// ADB mode. No-op if the device is already in ADB mode.
pub fn enable_command_mode() -> Result<()> {
    if open_orbic()?.is_some() {
        eprintln!("Device already in command mode.");
        return Ok(());
    }

    let timeout = Duration::from_secs(1);

    if let Some(device) = crate::orbic::usb::open_usb_device(VENDOR_ID, PRODUCT_ID_RNDIS)? {
        let req = Control {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0xa0,
            value: 0,
            index: 0,
        };
        let interface = device
            .detach_and_claim_interface(RNDIS_INTERFACE)
            .context("Failed to claim RNDIS interface")?;
        if let Err(e) = interface.control_out_blocking(req, &[], timeout) {
            if e == nusb::transfer::TransferError::Stall {
                return Ok(());
            }
            bail!("Failed to send mode-switch control request: {e}");
        }
        return Ok(());
    }

    bail!("{NOT_FOUND}");
}

/// Wait for the device to present as ADB-capable and return a handle.
pub async fn get_adb() -> Result<ADBUSBDevice> {
    const MAX_FAILURES: u32 = 10;
    let mut failures = 0;
    loop {
        match ADBUSBDevice::new(VENDOR_ID, PRODUCT_ID) {
            Ok(dev) => match adb_echo_test(dev).await {
                Ok(dev) => return Ok(dev),
                Err(e) => {
                    failures += 1;
                    if failures > MAX_FAILURES {
                        return Err(e);
                    }
                    sleep(Duration::from_secs(1)).await;
                }
            },
            Err(RustADBError::IOError(e)) if e.kind() == ErrorKind::ResourceBusy => {
                bail!("{BUSY}")
            }
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Err(RustADBError::IOError(e)) if e.kind() == ErrorKind::PermissionDenied => {
                bail!("{BUSY_MAC_WIN}")
            }
            Err(RustADBError::DeviceNotFound(_)) => {
                tokio::time::timeout(
                    Duration::from_secs(30),
                    wait_for_usb_device(VENDOR_ID, PRODUCT_ID),
                )
                .await
                .context("Timeout waiting for Orbic to reconnect")??;
            }
            Err(e) => {
                failures += 1;
                if failures > MAX_FAILURES {
                    return Err(e.into());
                }
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

// ── Rootshell setup ──────────────────────────────────────────────────────────

/// Push `rootshell_binary` to `/bin/rootshell`, set setuid root (4755), and
/// verify `id` returns uid=0.
async fn setup_rootshell(device: &mut ADBUSBDevice, binary: &[u8]) -> Result<()> {
    install_file(device, "/bin/rootshell", binary).await?;
    sleep(Duration::from_secs(1)).await;
    adb_at_syscmd(device, "chown root /bin/rootshell").await?;
    sleep(Duration::from_secs(1)).await;
    adb_at_syscmd(device, "chmod 4755 /bin/rootshell").await?;
    let output = adb_command(device, &["/bin/rootshell", "-c", "id"])?;
    if !output.contains("uid=0") {
        bail!("rootshell did not return uid=0 — setuid may have failed");
    }
    Ok(())
}

// ── AT+SYSCMD ────────────────────────────────────────────────────────────────

/// Send `AT+SYSCMD=<command>` over the USB serial interface.
pub async fn adb_at_syscmd(device: &mut ADBUSBDevice, command: &str) -> Result<()> {
    adb_serial_cmd(device, &format!("AT+SYSCMD={command}")).await
}

async fn adb_serial_cmd(device: &mut ADBUSBDevice, command: &str) -> Result<()> {
    let data = format!("\r\n{command}\r\n");
    let timeout = Duration::from_secs(2);
    let mut response = [0u8; 256];

    device
        .get_transport_mut()
        .send_usb_class_control_msg(INTERFACE, 0x22, 3, 1, &[], timeout)
        .context("Failed to send serial port control request")?;

    device
        .get_transport_mut()
        .usb_bulk_write(INTERFACE, 0x2, data.as_bytes(), timeout)
        .context("Failed to write AT command")?;

    // Consume echoed command
    device
        .get_transport_mut()
        .usb_bulk_read(INTERFACE, 0x82, &mut response, timeout)
        .context("Failed to read echoed command")?;

    // Read actual response
    device
        .get_transport_mut()
        .usb_bulk_read(INTERFACE, 0x82, &mut response, timeout)
        .context("Failed to read AT response")?;

    let s = String::from_utf8_lossy(&response);
    if !s.contains("\r\nOK\r\n") {
        bail!("Unexpected AT response: {s}");
    }
    Ok(())
}

// ── File transfer ────────────────────────────────────────────────────────────

async fn install_file(device: &mut ADBUSBDevice, dest: &str, payload: &[u8]) -> Result<()> {
    const MAX_RETRIES: u32 = 5;
    let mut attempts = 0;
    loop {
        match install_file_once(device, dest, payload).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                attempts += 1;
                if attempts > MAX_RETRIES {
                    return Err(e);
                }
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn install_file_once(device: &mut ADBUSBDevice, dest: &str, mut payload: &[u8]) -> Result<()> {
    let file_name = Path::new(dest)
        .file_name()
        .ok_or_else(|| anyhow!("{dest} has no filename component"))?
        .to_str()
        .ok_or_else(|| anyhow!("{dest} filename is not valid UTF-8"))?
        .to_owned();

    let tmp = format!("/tmp/{file_name}");

    let mut hasher = Sha256::new();
    hasher.update(payload);
    let expected_hash = format!("{:x}", hasher.finalize());

    device.push(&mut payload, &tmp)?;
    adb_at_syscmd(device, &format!("mv {tmp} {dest}")).await?;

    let info = device.stat(dest).context("Failed to stat transferred file")?;
    if info.file_size == 0 {
        bail!("File transfer unsuccessful: file is empty");
    }

    let output = adb_command(device, &["sha256sum", dest])?;
    if !output.contains(&expected_hash) {
        bail!("SHA256 mismatch — expected {expected_hash}, got: {output}");
    }
    Ok(())
}

// ── ADB helpers ──────────────────────────────────────────────────────────────

fn adb_command(device: &mut ADBUSBDevice, command: &[&str]) -> Result<String> {
    let mut buf = Vec::new();
    device.shell_command(command, &mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

async fn adb_echo_test(mut device: ADBUSBDevice) -> Result<ADBUSBDevice> {
    const ECHO_STR: &str = "orbic-toolkit-test";
    let thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        device.shell_command(&["echo", ECHO_STR], &mut buf)?;
        Ok::<(ADBUSBDevice, Vec<u8>), RustADBError>((device, buf))
    });
    sleep(Duration::from_secs(1)).await;
    if thread.is_finished() {
        if let Ok(Ok((dev, buf))) = thread.join() {
            if std::str::from_utf8(&buf).map(|s| s.contains(ECHO_STR)).unwrap_or(false) {
                return Ok(dev);
            }
        }
    }
    bail!("ADB echo test failed — try disconnecting and reconnecting the device")
}

// ── USB device enumeration ───────────────────────────────────────────────────

/// Get an nusb `Interface` for the Orbic (supports both ADB and combo PID).
pub fn open_orbic() -> Result<Option<Interface>> {
    if let Some(d) = open_usb_device(VENDOR_ID, PRODUCT_ID)? {
        return Ok(Some(
            d.detach_and_claim_interface(INTERFACE)
                .context("Failed to claim ADB interface")?,
        ));
    }
    if let Some(d) = open_usb_device(VENDOR_ID, PRODUCT_ID_COMBO)? {
        return Ok(Some(
            d.detach_and_claim_interface(INTERFACE)
                .context("Failed to claim combo interface")?,
        ));
    }
    Ok(None)
}

pub fn open_usb_device(vid: u16, pid: u16) -> Result<Option<nusb::Device>> {
    for info in nusb::list_devices()? {
        if info.vendor_id() == vid && info.product_id() == pid {
            return Ok(Some(info.open().context("Failed to open USB device")?));
        }
    }
    Ok(None)
}

// ── Hotplug watcher ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
async fn wait_for_usb_device(vendor_id: u16, product_id: u16) -> Result<()> {
    use nusb::hotplug::HotplugEvent;
    use tokio_stream::StreamExt;
    let mut watcher = nusb::watch_devices()?;
    while let Some(event) = watcher.next().await {
        if let HotplugEvent::Connected(dev) = event
            && dev.vendor_id() == vendor_id
            && dev.product_id() == product_id
        {
            return Ok(());
        }
    }
    bail!("USB hotplug watcher ended unexpectedly")
}

#[cfg(target_os = "macos")]
async fn wait_for_usb_device(vendor_id: u16, product_id: u16) -> Result<()> {
    // nusb::watch_devices does not work on macOS; poll instead
    loop {
        for info in nusb::list_devices()? {
            if info.vendor_id() == vendor_id && info.product_id() == product_id {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Send a raw AT command via an already-opened USB serial interface.
pub async fn send_serial_cmd(interface: &Interface, command: &str) -> Result<()> {
    let data = format!("\r\n{command}\r\n");
    let timeout = Duration::from_secs(2);

    let enable_serial = Control {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0x22,
        value: 3,
        index: 1,
    };

    interface
        .control_out_blocking(enable_serial, &[], timeout)
        .context("Failed to send serial port control request")?;

    tokio::time::timeout(timeout, interface.bulk_out(0x2, data.as_bytes().to_vec()))
        .await
        .context("Timed out writing AT command")?
        .into_result()
        .context("Failed to write AT command")?;

    // Consume echoed command
    tokio::time::timeout(timeout, interface.bulk_in(0x82, RequestBuffer::new(256)))
        .await
        .context("Timed out reading echoed command")?
        .into_result()
        .context("Failed to read echoed command")?;

    // Read actual response
    let response = tokio::time::timeout(timeout, interface.bulk_in(0x82, RequestBuffer::new(256)))
        .await
        .context("Timed out reading AT response")?
        .into_result()
        .context("Failed to read AT response")?;

    let s = String::from_utf8_lossy(&response);
    if !s.contains("\r\nOK\r\n") {
        bail!("Unexpected AT response: {s}");
    }
    Ok(())
}
