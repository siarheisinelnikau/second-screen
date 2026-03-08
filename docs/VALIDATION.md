# Validation record

Implementation session: 2026-10-31. The user authorized code, builds and focused tests on artificial data, and prohibited GUI launches, screenshots, real screen/device access, driver installation and remote publication.

## Completed checks

- Offline dependency resolution with `Cargo.lock` and repository-local build output; Cargo is limited to two jobs.
- `cargo clippy --locked --offline --all-targets -j 2 -- -D warnings`: passed, including the native event-loop adapter.
- `cargo test --locked --offline -j 2`: nine Rust tests passed, one loopback test intentionally ignored. The tests cover moving synthetic JPEG encode/decode, complete in-memory HTTP frame delivery, pairing and host approval, IP binding, token revocation, idle/code/approval expiry, brute-force throttling, request bounds and cross-origin/Host rejection, numeric version/asset selection, corrupted download rejection and persisted startup opt-out.
- `node --test tests/viewer.test.cjs`: two artificial-DOM tests passed. They exercise the approval gate, in-memory bearer header, image cleanup after revocation and a failed pairing request.
- `node --check viewer/viewer.js`: passed.

The full in-memory transport test invokes the real HTTP parser, router, response writer, synthetic capture worker, JPEG encoder and decoder. It does not use a simulated success response for authorization. None of these checks captures a real display.

## Environment restrictions encountered

- A real loopback TCP connection failed with Windows socket error 10013 (permission denied). The test listener was bound to `127.0.0.1:0`, never to LAN. No approval/elevation was requested. The opt-in test remains in source for an unrestricted environment.
- Atomic replacement of a test settings file in the system temporary directory was denied with OS error 5. The test now uses isolated `.test-output` fixtures inside this repository and passes there.
- The first shutdown implementation depended on a self-connection to wake `accept`. The denied connection exposed a shutdown hang; it was replaced with a `mio` selector and explicit waker. No self-connection is now needed for stopping the server. A real selector/socket shutdown check remains part of the opt-in test.
- The offline cache did not contain winit's optional default Linux decoration dependency. Direct winit uses the same minimal feature set as eframe instead; no dependency installation was needed.

## Manual checks still required

1. Launch the packaged EXE on Windows x64, inspect Connect/About/Updates, keyboard navigation, DPI scaling, clipping, minimize/restore and close. Capture real application and settings screenshots for the README.
2. Confirm no capture and no listener before Start. With a synthetic source, validate start, wrong/expired code, host approval, disconnect on either side, close, immediate rebind and overload/timeouts on loopback, then on a trusted LAN. Inspect the exact IPv4 and firewall behavior without granting broad network access.
3. Open the viewer on Safari/iPadOS and Chrome/Android; verify orientation, image decoding, full screen, tab backgrounding, loss/rejoin, status text, old-image removal and aspect ratio. No tablet/browser compatibility is asserted yet.
4. Only after that, choose Primary monitor and explicitly approve capture. Check normal desktop, cursor limitations, protected content, lock/unlock, DPI, resolution changes and error recovery. Confirm there is no false extended-monitor claim.
5. Measure release-build idle CPU with sharing off, listener idle, pending peer and streaming. Check pointer/window responsiveness and screen-to-tablet latency. There are no measured performance or latency claims.
6. With a disposable application folder and a future actual canonical release, validate startup update check, persistent opt-out with zero startup requests, manual check errors/rate limits, incompatible/missing assets, checksum failure, interrupted downloads, unwritable directories, helper readiness, replacement, rollback, backup recovery and relaunch. The updater has not been run.
7. Follow the virtual-display guide on a dedicated test machine before adding or advertising extension. No driver/device verification was performed here.
8. Review the generated third-party inventory before public distribution. Some upstream crate archives omit standalone license texts; the inventory identifies these gaps explicitly rather than inventing copyright notices. Complete those upstream notices and the bundled font attribution before publication.

CI is prepared for Windows formatting, Clippy, default artificial-data tests, viewer tests and static-runtime packaging. It performs no publication. Run the opt-in loopback test separately on a machine where local sockets are allowed.
