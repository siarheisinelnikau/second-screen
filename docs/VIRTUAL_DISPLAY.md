# Experimental virtual-display backend

## Implemented path and boundary

`driver/Driver.cpp` implements an IddCx/UMDF adapter and dynamically connected virtual monitor. `src/virtual_display.rs` implements the real Configuration Manager/device IOCTL transport, decoder and monitor lease. Build with `--features virtual-display`; default builds expose mirroring/synthetic sources only. The backend is **not hardware validated**. WDK is absent, so the complete driver build remains unverified. The host never installs/registers it.

```text
User approves paired tablet
 -> host opens SecondScreen interface and requests one monitor
 -> IddCx reports monitor arrival
 -> Windows activates/extends the target (user chooses arrangement)
 -> swapchain supplies that target's independent desktop surfaces
 -> two GPU staging slots -> latest packed BGRA mailbox
 -> owning host IOCTL -> RGB -> JPEG -> paired browser
Disconnect / file cleanup / reader timeout -> monitor departure
```

There is no physical-screen fallback. This supplies an independent target; the app does not change Windows topology. Actual extension must be selected and validated on a real installation before being advertised as working.

## Ownership

Interface GUID: `{89E573D9-1361-48D7-BD37-3C8E5598EF4A}`. Hardware ID: `Root\SecondScreenDisplay`. The host enumerates exactly one product interface and opens a non-inheritable read/write file without sharing. IOCTLs synchronously copy bounded CPU memory; they never wait on GPU completion or the network.

The INF ACL permits local interactive users, administrators and system. Frame access/removal additionally require the original owning WDF file object. Another handle cannot fetch or remove an occupied monitor. No user pointers, GPU handles or shared-memory names cross the protocol. Monitor creation occurs only on a valid owning request after adapter readiness, with a new generation and an EDID-less requested mode at 60 Hz. No real display EDID is copied.

File cleanup, D0 exit and a five-second no-reader watchdog request departure. A failed departure keeps the handle for retry. The capture source owns this lease and removes the monitor on drop, outside the UI/status mutex. Missing/incompatible drivers and GPU failures are reported instead of silently mirroring.

## Wire ABI v1

Little-endian fixed-width fields only: requests 32 bytes, reply header 64 bytes, no pointers/`usize`/handles.

| Operation | IOCTL | Request | Reply |
| --- | --- | --- | --- |
| Query | `0x0022e000` | Frame request, generation/sequence zero | Capabilities/readiness |
| Create | `0x0022e004` | Create request | Pending header with new generation/mode |
| Frame | `0x0022e008` | Owner generation and last sequence | Header and newest BGRA, or header only |
| Remove | `0x0022e00c` | Owner generation, sequence zero | Inactive header |

All use `FILE_DEVICE_UNKNOWN`, `METHOD_BUFFERED`, read/write access. C++ tests compare these constants with the real SDK `CTL_CODE`. Because buffered input/output can alias, the driver copies the whole input before writing output.

Create request: version `u32@0`, size `u32@4`, width `u32@8`, height `u32@12`, FPS `u32@16`, three zero reserved words `@20`. Bounds: width 320–1920, height 240–1080, FPS 5–20. Windows acceptance of lower mode bounds still needs device testing.

Frame request: version `u32@0`, size `u32@4`, generation `u64@8`, last sequence `u64@16`, reserved zero `u64@24`.

| Reply offset | Type | Meaning |
| --- | --- | --- |
| 0, 4 | `u32` each | Protocol `0x00010000`, header size 64 |
| 8, 16 | `u64` each | Generation, monotonically increasing sequence |
| 24, 28, 32, 36 | `u32` each | Width, height, packed stride, payload bytes |
| 40 | `u32` | Format 1 = BGRA8 |
| 44 | `u32` | 0 unchanged/pending, 1 frame, 2 inactive, 3 failed |
| 48 | `u32` | Failure HRESULT, otherwise zero |
| 52–63 | three `u32` | Reserved zeros |

A frame has exactly `width*height*4` payload bytes, stride `width*4`, matching generation/dimensions and a newer sequence. Rust rejects truncated/trailing data, oversized buffers, unknown states/formats, nonzero reserved fields and stale generations/sequences before converting BGRA to RGB. Pending/inactive replies have no pixels. Unchanged preserves the last frame; inactive clears host/browser images. Each reconnected monitor gets a new generation.

## Graphics and buffers

The swapchain worker creates a D3D11 BGRA device on IddCx's render-adapter LUID, sets the swapchain device and acquires its surfaces. Successful assignment transfers ownership; the worker deletes the swapchain exactly once at exit. Unassignment/monitor cleanup signals stop and joins it.

Readback has two fixed-size CPU-readable staging textures and completion queries. Submission validates format, dimensions and sample count, caps copies to requested FPS and skips when both slots are occupied. `GetData` and `Map(DO_NOT_WAIT)` avoid GPU completion waits. Older completed slots cannot overwrite newer frames. Row padding is stripped into one immutable latest BGRA frame; only bounded snapshots can exist. There is no frame history or network backpressure in the driver.

With no available surface, the worker waits on surface/stop events indefinitely unless a GPU readback is pending; only then is a 16 ms timeout needed to drain it. Unsupported format, GPU loss or readback errors surface as HRESULT errors to the host, which releases the monitor and asks for reconnect. HDR, cursor forwarding and hardware encoding are not implemented; performance remains unmeasured.

## Build and deployment status

See [build guide](../driver/README.md) and [validation record](DRIVER_VALIDATION.md). Driver source, INF and WDK project exist; no driver DLL/CAT has been built, signed or loaded. Build scripts never install, register, sign or create devices. Catalog/signing/deployment and Windows display arrangement require separate future user actions. Hardware validation is required before enabling this by default.
