# SecondScreen v0.2.0 — local implementation draft

Changes relative to the local mirroring-only v0.1.0 implementation. Neither version has been published to GitHub by this task.

## Added

- Experimental independent-display backend source: an original UMDF/IddCx adapter, dynamic monitor arrival/departure, requested modes, owner-file IOCTLs, swapchain processing and two-slot nonblocking D3D11 readback.
- A Rust provider that opens the product driver interface only after tablet approval, validates generations/strides/payloads and streams that display's frames through the existing JPEG viewer. Driver absence/failure gives an error, without falling back to a physical monitor.
- Monitor-lease cleanup on disconnect, file cleanup and power-down, plus a five-second reader watchdog and retained-handle departure retry.
- Unsigned build-only WDK project/INF, prerequisite checks and a manual build workflow for a prepared WDK runner. No install/sign/register commands are included.
- SDK-only mailbox tests and cross-language verification: Rust decodes the actual C++ mailbox's generated padded BGRA fixture. No device/GPU initialization is involved.

## Improved

- Connect now separates display choice, connection setup and pairing. Picture presets cover common settings; exact parameters remain in one expandable area.
- Removed frame/byte counters and the host's recurring one-second redraw timer. Native status wakes on meaningful changes; a synthetic regression checks that an ongoing stream does not wake it for every frame.
- The browser distinguishes mirroring, test output and experimental virtual displays, and clears old pixels when a target becomes inactive.
- Added bounded offscreen captures of the actual UI components, including narrow and missing-driver states. These verify layout with a disclosed CPU fallback, not native input/performance.

## Verification boundary

Rust and SDK-only C++ checks pass. The complete driver has not been compiled: genuine IddCx/UMDF WDK components are missing. No driver was installed, signed, registered or loaded; no desktop arrangement or other system settings changed. Actual extended output, native UI behavior, GPU operation, tablet compatibility and driver package lifecycle remain unverified.

The feature stays opt-in (`virtual-display` / `package.ps1 -VirtualDisplay`). Portable host asset: `SecondScreen-v0.2.0.exe`, Windows x64. The host EXE is separate from the driver package and cannot install it. HTTP remains unencrypted and restricted to a trusted LAN. See [driver validation](DRIVER_VALIDATION.md) and [design evidence](DESIGN.md).
