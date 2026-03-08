# Virtual-display integration boundary

## Current status

No virtual display is created by SecondScreen 0.1.0. No driver, INF, catalog, installation helper or signing certificate is shipped. The functional source is primary-monitor mirroring and a synthetic frame source. An abstract `ExtendedDisplayProvider` in `src/capture.rs` exposes `create_monitor(width, height)` returning a `FrameSource`, and `remove_monitor()`. It is intentionally not instantiated or selectable in the UI.

An ordinary user application cannot turn a JPEG stream into a Windows monitor by selecting a resolution. Microsoft documents IddCx as the UMDF driver model for remote/virtual display adapters, monitor arrival/departure, mode negotiation and receiving desktop frames. The source boundary is suitable for a later provider; it is not a substitute for that driver.

## Feasible separate WDK component

A future component should be a C++ universal UMDF 2 / IddCx driver project built with Visual Studio and matching Windows SDK/WDK versions, separate from Cargo and the portable host. Use Microsoft's [IDD model documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview) and [official IddSample](https://github.com/microsoft/Windows-driver-samples/tree/main/video/IndirectDisplay) as engineering references, observing the sample's own license if any code is reused. No competitor code or assets are needed.

The driver must implement WDF device/power/PnP lifecycle, asynchronous IddCx adapter initialization, monitor descriptor/default mode queries, target mode queries, monitor arrival/departure and swap-chain assignment/unassignment. A swap-chain worker receives DXGI surfaces, reports frame completion promptly, and tears down before device removal or mode changes. GDI/window APIs belong in the user session host, not in the driver.

Suggested host/provider contract:

- A versioned, local-only named-pipe control endpoint, ACL restricted to the owning interactive user and system. Authenticate the caller; never pass network input straight to driver control.
- Explicit create, resize and remove commands, with validated mode limits and one active monitor. A new monitor is created only on a user request after pairing. Do not silently change global display arrangement.
- Surface delivery through a bounded two-slot shared DXGI resource ring with negotiated adapter LUID, width, height, pixel format, frame sequence and keyed synchronization. The host copies/encodes only the newest available frame. Do not block the IddCx swap chain on network clients.
- A generation identifier invalidates frames across resize/unpair/device removal. Host exit, broken control pipe, sleep and timeout must release the monitor without leaving a phantom display.
- `FrameSource` owns the host-side connection. `ExtendedDisplayProvider::remove_monitor` performs explicit cleanup; its concrete implementation also needs a Drop fallback. Do not expose an "Extend" UI choice until the concrete provider can create, capture and remove a real monitor.

## Build and deployment work still required

1. Create the actual UMDF project and product-specific hardware ID, INF, monitor descriptor and supported mode set. Build x64 Debug/Release with MSBuild from a WDK developer environment; the repository currently contains no such project, so no driver build command can honestly succeed here.
2. Implement the callbacks, driver/host IPC, ownership and the Rust concrete provider. Add synthetic surface fixtures for ring overflow, mode-generation mismatch and cleanup before hardware tests.
3. Validate the complete driver package using WDK tooling and Driver Verifier on a disposable test machine. Test-signed development packages require explicit machine configuration; this session did not sign, register or install anything.
4. Complete the Windows driver signing/distribution process appropriate to the target Windows versions. A portable EXE does not avoid driver deployment privileges or signing requirements. Keep driver installation a separate explicit, reviewable user action.
5. Test real extended-desktop arrangement, hotplug, sleep/resume, DPI, locked sessions, GPU changes, tablet loss, uninstall and recovery. Only then label extension as supported.

This guide records a build plan for a missing integration component, not a claim of driver implementation. TLS, hardware encoding and tablet interaction are independent follow-up work.
