# Architecture

## Runtime ownership

The native egui/winit software interface sends bounded commands to a host-controller worker. That worker owns the active `Host`, starts it only after a user action, and joins it on stop/exit. Socket binding, capture, encoding, downloads and shutdown do not run in egui callbacks. Settings writes also run on the host-controller worker. A few bounded settings/status reads occur during startup.

The host binds one explicit private IPv4 address. A `mio` OS selector sleeps waiting for incoming connections and an explicit shutdown waker. Four HTTP workers share an eight-socket queue. Accepted sockets use blocking I/O with two-second read/write timeouts and a total request parsing deadline. Overload closes excess connections. Headers are limited to 8 KiB/32 fields and bodies to 1 KiB. Duplicate headers and transfer encodings are rejected; persistent HTTP connections are intentionally not supported.

The capture worker waits on a condition variable until a viewer is approved. GDI resources are acquired on that worker and stay there. JPEG encoding happens outside the shared state lock. One latest compressed frame is retained, with a maximum 8 MiB payload. Consumers clone an `Arc` instead of copying the JPEG. An in-flight frame is published only if its original session is still approved. No disk recording is performed.

The primary monitor is scaled into a compatible bitmap using `StretchBlt`, read as top-down BGRA and converted to RGB for JPEG. A changed display aspect/size causes a clear restart-required error rather than unsafe buffer reuse. This conservative GDI path is intended for ordinary desktop content; secure/protected content, cursor visibility, display changes and performance need manual validation. There is no GPU encoder or low-latency video codec.

The native interface repaints in response to input and meaningful worker state changes. A newer streamed image alone does not redraw the host controls. First frame, inactive source, capture error, pairing, expiry and disconnect wake the UI. `src/host.rs` consumes finite repaint deadlines and selects `Wait` when none remain. Unchanged OS expose events present a retained bitmap without rebuilding UI or rasterizing. Minimized/occluded windows process host/update logic but never layout, rasterize or present; data remains dirty until restore. No periodic UI timer exists. Capture keeps its useful 5–20 FPS deadline separately. No peer means no capture timer; pending peers/errors use bounded expiry waits.

Production rendering is egui 0.33.3 -> `egui_software_backend` 0.0.2 CPU RGBA rasterization -> softbuffer 0.4.8 Win32 GDI bitmap presentation. OpenGL/eframe dependencies were removed. The driver necessarily uses D3D11 for its IddCx surfaces, independently of the control window. Aggregate counters can be saved on exit with `SECOND_SCREEN_RENDER_METRICS`. Source scheduling and isolated fixtures are checked; runtime visible/hidden/restore frame counts and load remain unmeasured because native GUI launches are prohibited in this session.

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

The application's update worker and UI retain product preferences and explicit actions. Shared `rust-auto-updator` owns stable-release selection, canonical repository/asset validation, HTTPS, throttled progress, SHA-256/PE/size verification, staging and recoverable same-volume executable replacement. It is provisioned as a sibling path dependency and does not own the renderer, settings or host lifecycle.

Startup opt-out is read before enqueueing a background check; disabling it makes no startup request. Manual checks remain available. Startup failures stay in Updates without modal dialogs; an available update adds a persistent notification. Download/install starts only from the user's action. The release contract keeps `SecondScreen-vVERSION.exe` plus `SHA256SUMS`, or a valid GitHub SHA-256 digest for earlier digest-only metadata. Missing/corrupt verification data fails safely.

The downloaded executable now handles the shared `--apply-update` plan protocol before constructing the GUI. Prepared downloads clean staging files when dropped before handoff. The helper validates the parent process and target, waits for exit, checks installed bytes again, atomically replaces with a backup, rolls back a failed restart and passes `--cleanup-update` to the successfully restarted host. Host commands finish and sharing stops during shutdown; settings files are never replaced. Previously installed versions use their own copied old helper for the transition into this new version, so their helper protocol remains self-contained.

## Primary references

- [Microsoft: capturing an image with GDI](https://learn.microsoft.com/en-us/windows/win32/gdi/capturing-an-image)
- [Microsoft: indirect display driver model](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview)
- [GitHub: Releases REST API](https://docs.github.com/en/rest/releases/releases)
- Pinned crate sources in `Cargo.lock` establish the rendering and input APIs.
