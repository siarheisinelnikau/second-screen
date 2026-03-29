# SecondScreen

An independent Windows host and embedded tablet/browser viewer for a local desktop mirror. Rust, native desktop controls, one portable EXE, no account or cloud relay. MIT licensed; not affiliated with Duet.

**Version 0.1.0 is mirroring, not an extended desktop.** It does not create a Windows monitor. The Windows capture implementation is compiled, but actual screen capture and tablet operation have not yet been exercised. See [manual validation](docs/VALIDATION.md).

## What is implemented

- Windows primary-monitor capture through GDI, aspect-preserving scaling, JPEG encoding and authenticated HTTP frame delivery. A moving synthetic source exercises the same encoder and transport without reading the screen.
- Embedded responsive viewer with a pairing form, host approval, connection state, full screen, resolution indicator and disconnect. No browser installation, external scripts, fonts or CDN.
- One viewer per host, a random 10-character pairing code, five pairing attempts per minute globally, a 256-bit session token bound to the viewer IP, a fixed approval deadline and idle expiry.
- Native source, resolution, JPEG quality and 5–20 FPS controls. Start, approve, disconnect, rotate code and stop actions are explicit. Closing the host shuts down the listener and capture worker.
- Updates settings with the installed version, manual **Check for updates**, a persistent startup-check opt-out and **Download and install** with progress, digest checking, replacement helper and restart.

**Network limitation:** the viewer uses unencrypted HTTP. Pairing prevents casual unauthorized viewing, but does not protect frames or credentials from network observers or an active attacker. Use a trusted private LAN only. There is no TLS, internet relay, USB transport, remote input or unattended sharing. Do not port-forward this service. The host requires acknowledgment of this limitation before starting.

## Build

Windows x64 is the supported packaging target. Use a current stable Rust MSVC toolchain and the Visual C++ build tools. Node 22+ is used only for the viewer tests; it is not needed by the installed application.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -j 2 -- -D warnings
cargo test --locked -j 2
node --test tests/viewer.test.cjs
./scripts/package.ps1 -Offline
```

Omit `-Offline` on a clean development computer that needs dependencies. The repository configuration limits Cargo to two jobs and keeps build output in its own `target` directory. Packaging links the MSVC runtime statically and produces `dist/SecondScreen-v0.1.0.exe` and `dist/SHA256SUMS`. No installer, service or display driver is included. The executable embeds the viewer; ship the accompanying license notices as well.

`./scripts/generate-notices.ps1 -Offline` regenerates the locked Windows dependency inventory and available code/font license texts. The inventory explicitly marks upstream archives missing standalone code-license texts. Resolve those notices and review font attribution before public distribution; this local build is not a publication-ready release.

## Use after manual validation

1. Launch the packaged EXE. Sharing starts off; startup update checking runs in the background unless disabled in **Updates**.
2. In **Connect**, select **Synthetic test pattern** first. Enter the computer's RFC1918 private LAN IPv4 address. The default `127.0.0.1` is local to the host and cannot be opened on a tablet. IPv6, hostnames and wildcard binding are not supported.
3. Choose the port, maximum resolution, quality and FPS limit. Acknowledge the HTTP limitation and click **Start sharing**. Firewall access, if needed, must be configured by the user; the application does not alter firewall rules.
4. On a tablet on the same trusted network, open the exact `http://IP:PORT` address shown. Enter the code within five minutes. Check the requesting IP on the host and click **Approve viewer and start capture**.
5. For actual mirroring, stop, choose **Primary monitor**, start again and pair again. Only approval starts capture. The configured dimensions are limits, not a change to the Windows display mode; the native aspect ratio is retained without upscaling.
6. Use **Disconnect** in either application to revoke access, or **Stop sharing** to close the listener. The last frame is cleared on disconnect. An abandoned viewer expires after 15 seconds without authenticated requests; a pending request has a 60-second maximum approval window.

Settings are saved by **Save settings** and immediately when changing the startup-update toggle. Storage: `%LOCALAPPDATA%/SecondScreen/settings.json`. Tokens and pairing codes are never persisted. Stream changes apply on the next start. A browser reload requires pairing again.

## Updates and distribution status

The planned canonical repository is [siarheisinelnikau/second-screen](https://github.com/siarheisinelnikau/second-screen). It has **not been published by this task**. A missing repository/release, network error or rate limit is shown as an unavailable check, not as an up-to-date installation. Startup failures remain on the Updates page without a dialog.

Only stable semantic versions and the official Windows x64 asset `SecondScreen-vVERSION.exe` are accepted. The GitHub asset's SHA-256 digest is required, HTTPS redirects stay HTTPS, file size is bounded and the PE architecture is checked. Installation is always initiated by the user. Keep the portable EXE in a writable folder: the application will not elevate. The helper waits for a verified parent process to exit before replacing it, keeps a recovery backup, attempts rollback if replacement/relaunch fails and leaves settings untouched. The installed filename is preserved. See [update design and recovery](docs/ARCHITECTURE.md#portable-updates).

## Extended display status

An additional independent Windows desktop needs a real virtual-display backend. The current host has a `FrameSource` / `ExtendedDisplayProvider` boundary, but ships no IddCx driver and must not be advertised as desktop extension. The [virtual-display integration guide](docs/VIRTUAL_DISPLAY.md) covers the required WDK project, monitor lifecycle, surface transport, signing and validation work.

## Screenshots

Real screenshots of the Connect screen, Updates/settings screen and a connected tablet are **pending**. This implementation session prohibited launching GUI applications, capturing the screen or using real devices. No mock screenshots or broken image placeholders are supplied.

## Verification

The synthetic tests cover pairing/approval, token revocation, expiry, request bounds, cross-origin/Host rejection, JPEG encode/decode, the complete in-memory HTTP route and persistent update opt-out. Node tests exercise the viewer's approval gate, Authorization header and frame cleanup with an artificial DOM. A separate loopback TCP integration test is opt-in because this development sandbox denied loopback connections:

```powershell
cargo test --locked --lib loopback_synthetic_transport -- --ignored --nocapture
```

That command uses only loopback and synthetic pixels, never LAN binding or screen capture. Compilation alone does not establish Windows capture, browser compatibility, idle CPU use or installation correctness. See [validation status](docs/VALIDATION.md), [architecture](docs/ARCHITECTURE.md), [release-note draft](docs/RELEASE_NOTES.md), and [third-party notices](THIRD_PARTY_NOTICES.md).

The local release build succeeded (8,125,440-byte EXE). `python scripts/inspect_pe.py dist/SecondScreen-v0.1.0.exe` verifies the x64 GUI PE headers, import table and digest without executing it. Python is only an optional development verification tool. A separate opt-in shutdown test passed with a synthetic loopback listener and no client connection.
