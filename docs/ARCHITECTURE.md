# Architecture

## Runtime ownership

The native egui/eframe interface sends bounded commands to a host-controller worker. That worker owns the active `Host`, starts it only after a user action, and joins it on stop/exit. Socket binding, capture, encoding, downloads and shutdown do not run in egui callbacks. Settings writes also run on the host-controller worker. A few bounded settings/status reads occur during startup.

The host binds one explicit private IPv4 address. A `mio` OS selector sleeps waiting for incoming connections and an explicit shutdown waker. Four HTTP workers share an eight-socket queue. Accepted sockets use blocking I/O with two-second read/write timeouts and a total request parsing deadline. Overload closes excess connections. Headers are limited to 8 KiB/32 fields and bodies to 1 KiB. Duplicate headers and transfer encodings are rejected; persistent HTTP connections are intentionally not supported.

The capture worker waits on a condition variable until a viewer is approved. GDI resources are acquired on that worker and stay there. JPEG encoding happens outside the shared state lock. One latest compressed frame is retained, with a maximum 8 MiB payload. Consumers clone an `Arc` instead of copying the JPEG. An in-flight frame is published only if its original session is still approved. No disk recording is performed.

The primary monitor is scaled into a compatible bitmap using `StretchBlt`, read as top-down BGRA and converted to RGB for JPEG. A changed display aspect/size causes a clear restart-required error rather than unsafe buffer reuse. This conservative GDI path is intended for ordinary desktop content; secure/protected content, cursor visibility, display changes and performance need manual validation. There is no GPU encoder or low-latency video codec.

The native interface repaints in response to input and meaningful worker state changes. Frame counters and the former once-per-second UI timer were removed: a newer image alone does not redraw the host controls. First frame, inactive source, capture error, pairing, expiry and disconnect wake the UI. Its winit adapter replaces residual `Poll` with `Wait` after a batch while preserving `WaitUntil` deadlines and queued redraws. Capture has its own useful 5–20 FPS deadline. No peer means no capture timer; pending peers/errors use bounded expiry waits. Normal idle/hidden frame and CPU behavior remain unmeasured because native GUI testing is prohibited in this session.

The production control window still uses eframe's OpenGL renderer. The driver necessarily uses D3D11 for its IddCx surfaces; that is independent of UI presentation. Source-level scheduling is reviewed, but the native rendering/load gate remains incomplete. No low-CPU/GPU claim is made. `examples/ui_preview.rs` renders the same `ui.rs` components through a CPU fallback without an OS window; its PNGs verify layout only and cannot establish production backend parity or hidden-window efficiency.

## Experimental virtual source

With `virtual-display`, source activation after viewer approval opens the product device interface and requests a monitor through versioned IOCTLs. The source owns the monitor lease/file. GPU staging/readback and a latest-frame mailbox live in the driver; RGB conversion/JPEG encoding remain in the host. Pending output does not halt capture, inactive output clears the retained image, and failure releases the source. Source drop/removal runs outside the shared UI/status mutex. The driver watchdog removes a monitor without a reader after five seconds. See [driver protocol/lifecycle](VIRTUAL_DISPLAY.md) and [the WDK verification boundary](DRIVER_VALIDATION.md).

## Pairing and HTTP protocol

| Route | Purpose | Authorization |
| --- | --- | --- |
| `GET /`, `/viewer.js`, `/style.css` | Embedded static viewer | Public on the selected interface |
| `POST /api/pair` | JSON `{ "code": "..." }`, returns a random token | Pairing code, five global attempts/minute |
| `GET /api/state` | Approval state, source, FPS and capture error | Bearer token and original IP |
| `GET /api/frame?after=N` | Latest JPEG, or 204 if no newer frame | Approved bearer token and original IP |
| `POST /api/disconnect` | Revoke token, clear frame, rotate code | Bearer token and original IP |

Every request must use the exact literal `Host: IP:PORT`. An Origin header, when present, must match that HTTP origin. JSON content type is mandatory for pairing. The host sends no CORS permissions, disables caching, blocks framing with CSP and avoids external resources. Tokens live only in the viewer's memory and the host's memory, never URLs, cookies, logs or local storage. They are bound to an IP, not to a device identity.

Correct code entry creates a pending session; it does not authorize frames. The host must approve. Pending authorization expires after 60 seconds even with heartbeats. All sessions expire after 15 seconds without authenticated requests. Host/viewer disconnect rotates the pairing code; rate limiting survives a disconnect. A restarted host has fresh credentials. The code has ten random characters from a 31-character unambiguous alphabet, and expires five minutes after issuance. A new code can be requested on the host.

The viewer fetches a frame only after approval. It awaits image decoding before requesting the next one, revokes old object URLs and clears the image when disconnected or unauthorized. It maintains no frame queue and does not forward input. Background mobile tabs may be throttled and consequently expire. Full screen depends on the browser's permission/support.

HTTP provides **no confidentiality or server identity**. An observer can steal a token or code; an active network attacker can replace the viewer JavaScript. Pairing and same-origin checks do not solve that limitation. Trusted isolated networking is the current deployment boundary. TLS with a usable tablet certificate-trust setup should precede broader deployment.

## Portable updates

A separate update worker performs unauthenticated HTTPS checks against the canonical GitHub releases API with a 15-second request budget. It reports unavailable/error separately from current. The saved startup opt-out is read before enqueueing the startup check; a disabled toggle results in no startup update request. Manual checks remain available. Startup failures never open a dialog, and only an available update adds a persistent notification in the native interface.

The stable channel excludes drafts/prereleases, compares numeric semantic versions and checks the exact versioned asset name and canonical URL. Installation currently supports Windows x64 only. Asset metadata must provide a valid SHA-256 digest. A missing digest disables automatic installation; the application does not silently skip integrity verification. GitHub's digest is integrity metadata from the same HTTPS trust chain, not a separately trusted publisher signature.

After **Download and install**, a worker downloads at most 200 MiB into a unique staging file next to the portable EXE, reports byte progress, verifies size, SHA-256 and PE x64 header, then copies the currently installed executable to a unique helper file. Command arguments use `Command`, not a shell. The helper validates sibling paths and checks the parent process's executable path. A readiness handshake keeps the parent alive until the helper owns a process handle and is ready to wait for exit. Only then does the native interface close.

After exit, the helper rechecks the staged digest, renames the original to a unique `.previous-...exe` backup, installs the new file and relaunches it. Failed replacement/relaunch attempts restore the backup; failure messages are written to `%LOCALAPPDATA%/SecondScreen/update-error.txt` and shown under Updates on the next launch. Settings and user data are outside the executable directory and are not modified by the helper. If a filesystem or security product prevents even rollback, use the retained backup manually. No elevation is attempted.

The helper and backup are deliberately retained. After a verified restart the user may remove `.second-screen-helper-*.exe`, old `.previous-*.exe` backups and abandoned `.second-screen-update-*.exe` files from this application's folder. A download interrupted by closing the application may leave an incomplete staging file, which is never auto-executed. No automatic installation occurs at startup. Installer/file replacement, failure recovery and restart remain untested on real executables.

## Primary references

- [Microsoft: capturing an image with GDI](https://learn.microsoft.com/en-us/windows/win32/gdi/capturing-an-image)
- [Microsoft: indirect display driver model](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview)
- [GitHub: Releases REST API](https://docs.github.com/en/rest/releases/releases)
- Pinned crate sources in `Cargo.lock` were used to check Rust API details.
