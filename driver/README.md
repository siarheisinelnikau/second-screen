# SecondScreen virtual-display driver (experimental)

This directory contains an original C++ UMDF 2 / IddCx driver and its Windows x64 project. It reports one EDID-less monitor only after an owning host file explicitly requests it. An activated Windows virtual monitor receives its own desktop surfaces. There is no automatic primary-monitor fallback, driver installation, signing or desktop topology change.

**Status:** the Rust bridge, protocol/mailbox tests and SDK-only GPU readback compilation have been checked. The complete WDK driver has not been compiled or loaded: the machine lacks IddCx/UMDF headers, libraries and WDK targets. No driver DLL or catalog is claimed to exist.

## Sources

- `Driver.cpp`: adapter/monitor lifecycle, modes, owner-file IOCTLs, a five-second reader watchdog and swapchain worker.
- `GpuReadback.*`: render-adapter selection, two D3D11 staging slots, completion queries and nonblocking readback.
- `FrameMailbox.hpp`: immutable latest frame; strips row padding, handles generations, sequence, suspension/failure/close. No frame history.
- `Protocol.hpp`: pointer-free ABI matching `src/virtual_display.rs`.
- `SecondScreenDisplay.vcxproj`, `.inf`: x64 Universal UMDF/IddCx package sources with a unique hardware ID/interface. Signing/catalog generation are disabled.
- `tests/CoreTests.cpp`: generated pixels only; no device or GPU calls.

## Build without deployment

Use a WDK-compatible Visual Studio 2022 C++ installation, SDK/WDK 10.0.26100.0, UMDF 2.25, IddCx 1.4 and its user-mode driver toolset. The INF targets Windows 11 x64 build 22000 or newer, not yet a tested compatibility claim.

```powershell
./scripts/build-driver.ps1 -CheckOnly
./scripts/build-driver.ps1 -Configuration Release
```

The script uses MSBuild `/t:Build /m:2`, `SignMode=Off`, `EnableInf2Cat=false`; output stays under `target/driver/Release`. It never invokes installation, certificate, registry, boot-configuration or device-creation tools. Missing prerequisites cause an error; nothing is downloaded or installed. A manual CI workflow targets an explicitly provisioned `self-hosted, Windows, X64, wdk` builder.

The INF references a future catalog. Unsigned build output without a catalog is a build artifact, not an installable release package. Driver delivery needs a separately reviewed signing, installation and update lifecycle; the portable-host EXE updater cannot service a driver package.

## Safe checks

```powershell
./scripts/test-driver-core.ps1
cargo test --locked --offline --features virtual-display -j 2
cargo test --locked --offline --features virtual-display -j 2 decodes_actual_cpp_mailbox_fixture -- --ignored
```

The first command compiles `GpuReadback.cpp` to an object with existing SDK/MSVC tools and runs only the independent mailbox console test. It writes `.test-output/driver-frame.bin` from generated padded BGRA pixels. The final Rust test decodes that actual C++-produced frame and checks RGB values, channel order, bounds and sequence handling. No device interface or graphics device is opened.

## Host integration

```powershell
./scripts/package.ps1 -Offline -VirtualDisplay
```

This builds the portable host with an optional experimental source selector; it does not build/install/open the driver. Default builds remain mirroring/synthetic-only.

After separately authorized installation and device validation, select the experimental source, pair the tablet and approve **create virtual monitor**. Only then does the capture worker open the product interface, negotiate the ABI and request a monitor. Missing/incompatible drivers produce an error, without automatic installation or mirroring fallback. Select Extend for the new monitor manually in Windows during validation; the host never calls `SetDisplayConfig` or changes other displays. Width/height are the virtual mode; JPEG readback is 5–20 FPS and the display mode is 60 Hz.

Disconnect, stop, process/file cleanup, power-down or five seconds without frame reads remove the monitor. Failed departure retains the handle for watchdog retry. See [protocol/lifecycle](../docs/VIRTUAL_DISPLAY.md) and [validation status](../docs/DRIVER_VALIDATION.md). The feature remains experimental until a genuine WDK build and hardware run succeed.

API sequencing and project conventions were checked against Microsoft's [IddSample](https://github.com/microsoft/Windows-driver-samples/tree/main/video/IndirectDisplay), [IddCx objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/iddcx-objects) and [swapchain contract](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/iddcx/nc-iddcx-evt_idd_cx_monitor_assign_swapchain). No real-monitor EDID, branding or competitor code was copied. This source uses the repository's MIT license; installed Microsoft SDK/WDK components retain their terms.
