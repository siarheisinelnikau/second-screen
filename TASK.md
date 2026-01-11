# SecondScreen

Repository: `second-screen`
Priority: 7 of 15 (user-requested launch order)
Product: SecondScreen
Purpose: Use a tablet as an additional computer display
Inspiration: https://www.duetdisplay.com/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Build a Rust Windows host and a lightweight tablet/browser viewer for local-network display streaming with explicit pairing, a connection indicator, resolution/quality controls and a clear disconnect action. Prefer a host single EXE with embedded viewer assets. Implement real capture/transport/decoding paths in source and validate transport with synthetic generated frames only; do not capture the actual screen, open LAN services or launch a browser during this task.

An extended desktop is the desired end state. Assess the virtual-display requirement honestly: include a clean integration boundary and any feasible virtual-display component source/build documentation, but do not install/sign/register drivers or request admin access. A screen-sharing implementation must be labelled mirroring until a real extended-display backend is integrated and tested. Authentication/pairing and bounded buffers matter. Implement the useful source work available offline rather than stopping at a design document; report exactly which driver/device checks remain for the user.

## Packaging and publication target

Windows standalone asset: `SecondScreen-vVERSION.exe` (for the initial release, `SecondScreen-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/second-screen (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

