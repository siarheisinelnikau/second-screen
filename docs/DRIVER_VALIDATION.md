# Virtual-display validation — 2026-10-31

The user requested backend sources/build preparation without installation or system changes. No GUI, driver/device interface, real screen capture, graphics device or LAN service was opened. Output remains in this repository.

## Completed

- `./scripts/test-driver-core.ps1`: passed with existing MSVC 14.51.36231 and SDK 10.0.26100.0. Compiled real `GpuReadback.cpp` with `/W4 /WX`, without GPU initialization. Ran the mailbox fixture on generated pixels: ABI/SDK IOCTL constants, invalid modes/reserved fields, padded rows, short buffers, generations, sequence, suspend/resume and permanent closure.
- `cargo clippy --locked --offline --all-targets --features virtual-display -j 2 -- -D warnings`: passed for the integrated Rust backend/native selector.
- `cargo test --locked --offline --features virtual-display -j 2`: 12 passed, 3 opt-in cases ignored. New tests check hostile headers, idempotent monitor ownership/removal with fake transport, and status-only redraw notifications during a synthetic stream. No devices are opened.
- `cargo test --locked --offline --features virtual-display -j 2 decodes_actual_cpp_mailbox_fixture -- --ignored --nocapture`: passed. Rust decoded the real C++ mailbox's generated `.test-output/driver-frame.bin`, checking RGB values, channel order, dimensions and stale-sequence rejection.
- Three artificial-DOM viewer tests passed, including clearing an inactive virtual target before another frame request.

## Blocked WDK build

`./scripts/build-driver.ps1 -CheckOnly` reports missing WDK prerequisites, including:

- `C:/Program Files (x86)/Windows Kits/10/Include/10.0.26100.0/um/iddcx/1.4/IddCx.h`
- `C:/Program Files (x86)/Windows Kits/10/Include/wdf/umdf/2.25/wdf.h`
- WDK build targets/toolset and IddCx/UMDF libraries.

Windows SDK alone cannot build `Driver.cpp`. No stub headers were substituted to claim success. No WDK installation, elevation, signing, device registration or certificate operation was attempted. The WDF/IddCx callbacks, INF and complete linking remain unverified until built on a prepared WDK machine.

## Device checks still required

1. Compile the complete driver Debug/Release with WDK; run INF/static checks and resolve DDI/toolset errors before separately authorized signing/deployment.
2. On a disposable Windows 11 x64 test machine, confirm no monitor exists before create. After manually choosing Extend, verify an independent desktop and its own DXGI surface, not a physical-screen copy.
3. Check mode bounds/60 Hz timing, readback cap, padded pixels, static desktops, invalid modes, GPU reset and unsupported formats. HDR/cursor/performance compatibility is not asserted.
4. Verify owner-file/user isolation, private IOCTL access, malformed requests, second-client behavior, generations, create failures and partial initialization cleanup.
5. Check disconnect, stop, app exit/crash/hang, watchdog, failed departure retry, D0 transitions, sleep/resume and unassignment. GPU resources and the monitor must disappear without rearranging physical displays.
6. Validate tablets, UI mode labels and inactive-image clearing; capture real screenshots only when GUI tests are authorized.
7. Review signing/catalog, installation/update/uninstall and recovery separately. The portable updater does not install drivers.

Default builds remain mirroring/synthetic-only; the driver backend stays experimental and opt-in pending these checks.

## Design/rendering evidence

Applied `product-design` and the `rust-performance` source/load gate. Removed recurring UI status polling and frame metrics, kept capture/cleanup off the UI lock, and retained the reactive winit adapter. The production host is still eframe/OpenGL; no native launch or idle/hidden CPU/GPU measurement was authorized, so its runtime gate remains incomplete.

The bounded `ui_preview` console harness rendered six real-component fixture views without any window, device, network or application workers. PNGs were inspected with an image viewer; README uses the Connect and Updates client-area captures with explicit CPU-fallback scope. This is static layout evidence, not a native-renderer or hardware test. See [design record](DESIGN.md).

## Host package

`./scripts/package.ps1 -Offline -VirtualDisplay` passed, producing `dist/SecondScreen-v0.2.0.exe` (8,135,168 bytes). Static PE inspection passed: x64, Windows GUI subsystem, no dynamic MSVC runtime imports. SHA-256: `772a5a5a56300e645c0873fe6d85881c7571eddec9a284fd0cae1341de1640fb`, matching `dist/SHA256SUMS`. The host was not launched; the driver was not built or bundled.

Formatting and Clippy passed for default and virtual-display configurations. The generated third-party inventory includes build/test dependencies and currently marks 15 missing upstream standalone code-license texts (including a new preview-only dependency); these remain a publication prerequisite. This task does not publish a release.

The default feature configuration was also tested: 10 passed, 2 loopback cases ignored. The standalone C++→Rust fixture test passed separately; the full virtual-display suite has 12 passed and 3 opt-in cases ignored. No ignored socket/frame test was represented as a TCP success.
