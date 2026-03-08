# SecondScreen v0.1.0 — initial release draft

Local implementation draft. No GitHub repository, tag or release has been published by this task.

## Added

- A native Windows host with explicit start/stop, primary-monitor mirroring and a generated moving test-pattern source.
- A self-contained tablet browser viewer with pairing, host approval, connection status, full-screen presentation and disconnect. No account or external viewer assets are required.
- Resolution limits with aspect-ratio preservation, JPEG quality and bounded frame-rate controls.
- One-viewer authorization with expiring pairing codes, request throttling, in-memory bearer tokens, host-side revocation and bounded frame/socket queues.
- Persistent startup update checking preference, manual release checks and a user-initiated portable executable updater with progress and SHA-256 verification.
- Synthetic encode/decode, protocol, pairing, settings and browser-logic tests, plus an opt-in loopback TCP test.

## Requirements and known limitations

- Windows x64 portable asset: `SecondScreen-v0.1.0.exe`. Viewer assets and the static MSVC runtime are included; license notices accompany the EXE.
- Mirroring only. This release does not create a virtual monitor or extend the desktop. No display driver is included.
- Unencrypted HTTP on an explicitly selected trusted private IPv4 interface. No TLS, remote input, audio, USB, cloud relay or multi-viewer support.
- Actual Windows capture, firewall behavior, tablet compatibility, idle CPU/rendering, full-screen interaction, update replacement and recovery require manual verification before publication. Real application/settings screenshots are pending.
- The updater reports an unpublished/missing repository or release as unavailable. No real release check/download/install was executed during implementation.
