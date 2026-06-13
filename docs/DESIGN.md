# Connect workflow and static visual review

Applied the product-design and rust-performance skills during the 0.2.0 work. Normal use is: choose Mirror/Extend/Test, enter the computer's private address, choose a simple picture preset, start, then pair/approve the tablet. Listening is distinct from approval and actual desktop output. The current state, address, pairing code and explicit disconnect/stop actions take priority over technical counters.

The previous flat grid exposed all codec/network fields at once and refreshed frame/byte metrics every second. Presets now cover common picture choices; specialist parameters remain in one collapsing section. Connect/Updates/About retain one consistent entry point. A tray was considered but not added: pairing approval and a visible stop action matter for this early sharing utility, and closing the window continues to stop capture/listening rather than silently keeping a screen shared.

The palette is navy with a mint action accent, readable blue-gray secondary text, amber limitations and rose errors. Shared spacing and typography live in `ui.rs`. Status also has plain text, not color alone. Important buttons have readable labels and regular interaction height. The trust acknowledgment remains next to Start because HTTP confidentiality is a real limitation.

## Evidence and limitations

The `ui_preview` example uses the actual shared UI functions and fonts, not a separately drawn mockup. It performs six isolated offscreen scenarios, four frames each, saves PNGs, and exits. The script gives only that disposable console harness a 30-second deadline. The source path creates no native window, starts no application workers, opens no network/device interface, and reads no user settings. No computer use was performed.

The fallback is `egui_software_backend`; the production host remains eframe/OpenGL. This is layout evidence, not a claim of matching native rendering or satisfying the native idle/hidden performance gate. The driver D3D11 path was only compiled, never initialized for these pictures.

Inspected Connect (820×760), pairing, Updates, a narrow Connect (640×640), experimental Extend setup and missing-driver error. An initial narrow view hid Start below the fold. Reduced repeated text and spacing, then recaptured; the trust checkbox and primary action now fit at 640×640 with normal details collapsed. Detailed settings and smaller supported window heights remain scrollable. Native focus/keyboard traversal, scaling, minimize/restore, window chrome and performance are pending authorized GUI validation.

README images are component captures with explicit fixture/renderer scope. A real connected tablet is not pictured or claimed to work.
