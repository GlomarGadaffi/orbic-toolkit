# orbic-toolkit

**Universal root-access & payload installer for the Orbic RC400L mobile hotspot**

[![Build](https://img.shields.io/github/actions/workflow/status/GlomarGadaffi/orbic-toolkit/ci.yml?branch=main)](https://github.com/GlomarGadaffi/orbic-toolkit/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Orbic-toolkit gives you full root shell access on the Orbic RC400L ARMv7 Linux hotspot and a generic framework for deploying any ARM binary as a managed busybox `start-stop-daemon` service — without modifying the firmware or breaking the stock hotspot functionality.

It extracts and generalises the exploit chain pioneered by [EFF rayhunter](https://github.com/EFForg/rayhunter): authenticate against the device's web admin API, inject a root netcat shell via `SetRemoteAccessCfg`, then use that shell to push binaries and init scripts over a verified file transfer.

---

## Legal Disclaimer

> **USE THIS SOFTWARE AT YOUR OWN RISK.**
>
> This toolkit exploits a command-injection vulnerability in the Orbic RC400L admin interface to gain root access to a device **you own**. It does not target other people's devices or networks.
>
> We believe using this tool on your own device is lawful in the United States. However, **we are not lawyers and this is not legal advice.** Laws vary by jurisdiction. If you are outside the US, consult a qualified attorney before using this software.
>
> The authors accept no liability for bricked devices, voided warranties, legal consequences, or any other harm resulting from the use of this software.
>
> The USB install path on Windows is **not fully supported and may brick your device**. Use Linux or macOS for USB installs.

---

## Features

- **Two root access paths**
  - **Network** (recommended): WiFi-based HTTP exploit → persistent nc shell on port 24. No USB cable needed.
  - **USB**: Vendor control request → ADB → AT+SYSCMD. Optional setuid rootshell install for fully persistent root.
- **Generic payload framework**: describe any ARM binary + its runtime config in a simple `payload.toml`, and the toolkit installs it as a managed `start-stop-daemon` service with a proper busybox init script.
- **Full service lifecycle**: `install`, `uninstall`, `service start|stop|restart|status`, `list`.
- **Port conflict detection**: declares ports at install time and warns on conflicts.
- **Verified file transfer**: MD5 (network path) and SHA256 (USB path) on every pushed file. The asymmetry is transport-determined: the network path verifies using `md5sum` inside the device's busybox environment (the lowest-common-denominator hash available in a nc shell); the USB path computes SHA256 host-side via Rust's SHA2 crate before the ADB push.
- **Hotspot-safe**: all operations preserve stock 4G/LTE hotspot functionality.

---

## Supported Devices

| Device | Status |
|--------|--------|
| Orbic RC400L | ✅ Primary target |
| Kajeet RC400L | ✅ Same hardware, same exploit |

Other devices supported by rayhunter (TP-Link M7350, Moxee, TMOHS1, etc.) are **not yet supported** — see [Roadmap](#roadmap).

---

## Quickstart

### Prerequisites

- Rust toolchain (stable, 1.75+): [rustup.rs](https://rustup.rs)
- The device connected to your WiFi (network path) **or** via USB (USB path)
- Your Orbic admin password (find it on the label under the battery or in the web UI at `http://192.168.1.1`)

### Build

```sh
git clone https://github.com/GlomarGadaffi/orbic-toolkit
cd orbic-toolkit
cargo build --release
# Binary: target/release/orbic-toolkit
```

### Drop into a root shell (network)

```sh
orbic-toolkit --password <YOUR_PASSWORD> shell
```

The shell is a busybox nc shell — it's limited (no job control, no tab completion) but fully functional for all install operations.

### Drop into a root shell (USB)

```sh
orbic-toolkit --via usb shell
```

On Windows: you will be warned that USB installs are not fully supported and asked to confirm before proceeding.

### Install a payload

1. **Cross-compile your binary** for `armv7-unknown-linux-musleabihf` (see [Cross-Compilation](#cross-compilation))

2. **Write a `payload.toml`**:

```toml
name        = "my-tool"
version     = "1.0.0"
data_dir    = "/data/my-tool"
binary_name = "my-tool"
args        = "--port 9000"
log_file    = "/data/my-tool/my-tool.log"
pidfile     = "/tmp/my-tool.pid"
ports       = [9000]
```

3. **Install**:

```sh
orbic-toolkit --password <PASS> install payload.toml --binary ./my-tool-arm
```

The toolkit will:
- Exploit the device → open root shell
- Remount `/` as rw
- Push and verify the binary
- Render and install a busybox init script at `/etc/init.d/my-tool`
- Reboot (the service starts automatically on every boot)

### Uninstall

```sh
orbic-toolkit --password <PASS> uninstall my-tool
```

Stops the service, removes the init script, removes `/data/my-tool/`, and reboots.

### Other commands

```sh
# Run a one-off command
orbic-toolkit --password <PASS> run "uname -a"

# Push any file
orbic-toolkit --password <PASS> push ./local-file.txt /data/my-tool/config.txt

# Pull a file
orbic-toolkit --password <PASS> pull /data/my-tool/my-tool.log ./local-copy.log

# List installed services
orbic-toolkit --password <PASS> list

# Control a service
orbic-toolkit --password <PASS> service my-tool restart

# Open backdoor telnet without entering a shell
orbic-toolkit --password <PASS> start-telnet
```

---

## payload.toml Reference

| Field | Required | Description |
|-------|----------|-------------|
| `name` | ✅ | Service name; also used as the init script filename |
| `version` | ✅ | Your payload's version (informational) |
| `data_dir` | ✅ | Absolute path on device where the binary lives |
| `binary_name` | ✅ | Filename of the binary on the device |
| `args` | | Command-line args passed to the binary at start |
| `log_file` | ✅ | Where stdout/stderr is written |
| `pidfile` | ✅ | PID file path for start-stop-daemon |
| `ports` | | Runtime ports this service uses (conflict-checked at install) |
| `pre_start` | | Shell commands injected before `start-stop-daemon` in the `start` case |

---

## Port Registry

| Port | Use | Lifetime |
|------|-----|----------|
| 24 | Exploit nc shell | Ephemeral (install session only) |
| 8080 | rayhunter web dashboard | Persistent (if rayhunter installed) |
| 8081 | File transfer nc listener | Ephemeral (each file push) |
| 5060 | SIP (e.g. pocket-dial) | Persistent (your payload) |

Port 8081 is doubly used: as an ephemeral file-transfer listener during installs, and as a potential persistent port for payloads. It is free after each install session completes (before reboot).

---

## Cross-Compilation

The Orbic RC400L is `armv7-unknown-linux-musleabihf` (ARMv7 hard-float, musl libc, statically linked).

```sh
# Add the target (one-time)
rustup target add armv7-unknown-linux-musleabihf

# Install a musl cross toolchain (Linux)
# apt: apt install gcc-arm-linux-gnueabihf
# macOS: brew install arm-linux-gnueabihf-binutils

# Build
cargo build --release --target armv7-unknown-linux-musleabihf
```

Or use the [rayhunter Docker environment](https://github.com/EFForg/rayhunter) which already has the musl cross toolchain set up.

---

## How the Exploit Works

The Orbic RC400L exposes an unauthenticated admin API at `http://192.168.1.1`. The `SetRemoteAccessCfg` endpoint processes a `password` field without sanitisation, allowing shell metacharacter injection:

```json
{"password": "\"; busybox nc -ll -p 24 -e /bin/sh & #"}
```

This spawns a persistent listener on port 24 that gives a root `/bin/sh` shell to any connecting client. The toolkit uses this to push files and configure services, then closes the listener on reboot (it is not persistent across reboots by default).

Authentication uses a custom MD5 + base64 + character-swap encoding scheme described in [`src/orbic/auth.rs`](src/orbic/auth.rs).

Full credit to the [EFF rayhunter team](https://github.com/EFForg/rayhunter) for discovering and documenting this exploit chain.

---

## Roadmap

See [GitHub Milestones](https://github.com/GlomarGadaffi/orbic-toolkit/milestones) for the full tagged issue list.

| Milestone | Focus |
|-----------|-------|
| **v0.1** | Core install / uninstall / shell (current) |
| **v0.2** | Persistent shell — **key-authenticated Dropbear SSH only** (see note below) |
| **v0.3** | Cross-compile toolchain docs + Docker build env |
| **v0.4** | Additional device support (TP-Link M7350, Moxee, TMOHS1) |
| **v1.0** | Integration tests, CI pipeline, stable API |
| **Future** | OpenWRT-style full firmware replacement (very long roadmap) |

> **v0.2 security posture note.** Every operation in v0.1 opens port 24 only for the duration of one install session on your own LAN, and the listener is killed by reboot. That is the narrow-exposure posture and it is deliberate. v0.2 changes this: a Dropbear or telnetd init payload is a **standing root service that survives reboots**. That is a qualitatively different thing. Two hard constraints apply:
>
> 1. **Key-authenticated Dropbear only.** unauthenticated telnetd is explicitly out of scope for the built-in payload — it leaves an open root shell across reboots with no authentication boundary. If you install telnetd yourself via the generic payload framework, that is your choice and your risk; this toolkit will not ship it as a convenience payload.
> 2. **Installer requires an authorized public key.** `orbic-toolkit install-shell` will refuse to complete unless a public key is supplied via `--authorized-key`. There is no "skip key" flag.
>
> If you are using this on a device that shares a WiFi AP with other people or that you do not physically control, a persistent root listener is not appropriate regardless of authentication.

---

## Acknowledgements

- [Electronic Frontier Foundation](https://www.eff.org/) and the [rayhunter contributors](https://github.com/EFForg/rayhunter/graphs/contributors) for the original exploit research and installer framework that this toolkit is based on.
- The rayhunter project is licensed under GPL-3.0. orbic-toolkit is an independent re-implementation; no rayhunter source code is copied verbatim into this repository.

---

## Contributing

Issues and PRs welcome. Please tag issues with the appropriate labels (see the label taxonomy in the issue tracker). For security-sensitive findings, open a GitHub issue marked `type: security`.

## License

[MIT](LICENSE) — Copyright (c) 2025 GlomarGadaffi
