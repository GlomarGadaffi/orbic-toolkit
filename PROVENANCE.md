# Provenance

orbic-toolkit contains code ported from the rayhunter installer
(EFF rayhunter, `installer/src/`, GPL-3.0). Because of that, this project is
licensed under **GPL-3.0-only** (see [LICENSE](LICENSE)).

An earlier version of the README said the project was an independent
re-implementation with no rayhunter source copied verbatim. That statement was
wrong and has been removed.

## Per-file record

| File | Derived from (rayhunter @ 08a7c6a) | Evidence | Status |
|------|------------------------------------|----------|--------|
| `src/orbic/auth.rs` | `installer/src/orbic_auth.rs` | Header at line 24 says "Ported directly from". 10 of 18 unique lines of 45+ characters are identical. | Ported |
| `src/orbic/remote_access.rs` (formerly `exploit.rs`) | `installer/src/orbic_network.rs` | Header at line 23 says "Ported from". 12 of 39 identical lines, including the SetRemoteAccessCfg injection string at line 107 and the login error messages. | Ported |
| `src/orbic/usb.rs` | `installer/src/orbic.rs` | Header at line 3 says "Ported from". 23 of 118 identical lines. | Ported |
| `src/connection/telnet.rs` | `installer/src/util.rs` | 13 of 45 identical lines. No attribution in the file. | Partly copied |
| `src/connection/mod.rs` | — | 2 of 12 matching lines, both generic trait signatures. | Not treated as copied |
| `src/cli.rs`, `src/main.rs`, `src/ops/*`, `src/payload/*`, `src/orbic/mod.rs`, `dist/init.template.sh`, `Cargo.toml`, `README.md` | — | 0–2 matching lines each, all generic (imports, print strings, the adb_client git dependency). | No verbatim copying found. Structural similarity not assessed. |

## Method and limits

- Each source file was compared line by line with the rayhunter tree at commit
  `08a7c6a`. A line counts as matching if it is identical after trimming
  whitespace and is at least 45 characters long. Files searched: `*.rs`,
  `*.sh`, `*.toml`, `*.md`.
- Rewritten logic, renamed identifiers, and reordered code will not show up
  in this check. "No verbatim copying found" does not mean "independently
  written".
- No clean-room rewrite has been done. The code was written with rayhunter's
  source in view, so any rewrite made now would not be clean-room. GPL-3.0-only
  covers the existing code as it is.
- `adb_client` is pulled from the EFForg fork, which is MIT-licensed. MIT code
  can be combined into a GPL-3.0 work.

## Attestation

I have reviewed the statements above against the source. The owner of this
repository must review them and sign below. This file is not signed by the
tool that drafted it.

Attested by: ______________________________ (repository owner)

Date: ______________________
