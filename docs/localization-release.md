# SecondScreen v0.3.0

This is the first published release of SecondScreen.

## Initial capabilities

- Mirror the Windows primary monitor to the embedded tablet/browser viewer using a local authenticated connection and host approval.
- Choose picture presets and bounded frame-rate/network controls, with one paired viewer and an explicit disconnect workflow.
- Keep sharing off until requested. The optional virtual-display driver remains separate source with signing/deployment requirements and is not installed or bundled by the portable host.

## Localization and appearance

This release adds language and appearance preferences to the Windows host and embedded tablet viewer.

## Added

- English, Russian, French, German, Spanish and Italian interfaces, selected using native language names. Connect, pairing, settings, updates, About, help and ordinary status/error messages use the shared embedded catalog.
- A Settings page with saved Language and Light/Dark controls. Changing either preference applies immediately without restarting, disconnecting the tablet or changing the stream settings.
- Matching language and appearance selectors in the tablet viewer. Browser preferences are independent of the host and persist locally when browser storage is available.
- A light palette with readable text, forms, selection, warning and error colors; the existing dark palette remains the default.
- Explicit appearance choices remain selected when Windows changes its own light/dark mode.

## Improved

- Existing settings keep their connection, stream and startup-update choices when the new preference fields are absent. English is the complete fallback language.
- Parameterized messages preserve addresses, versions, dimensions and diagnostic details. Pending statuses are rendered in the current language after a language change.
- Preference saving uses the existing bounded host worker. Translation data is parsed once, and appearance changes only request the finite repaint needed to apply them; no new polling or recurring drawing is introduced.
- Viewer messages and static labels share the host catalog instead of keeping separate translation copies. Technical operating-system/network diagnostics retain their original details beneath localized context.

## Distribution and validation

Windows x64 portable asset: `SecondScreen-v0.3.0.exe`, with `SHA256SUMS`, `LICENSE` and third-party notices. The host EXE contains the viewer and its translations; no extra runtime locale files are required. Driver packages remain separate and the experimental driver source retains its independent version. Default builds support monitor mirroring and synthetic connection testing; experimental desktop extension still requires the opt-in feature and a separately built, signed, installed and validated driver.

Formatting, strict Clippy, Rust fixture tests, browser-viewer tests and package inspection are recorded in the current validation section of the README. Native-window and tablet interaction, runtime load counters, light/localized screenshots and real device/capture/update installation were not exercised in this code-only task. Existing v0.2.0 screenshots document the earlier English/dark interface.

HTTP remains unencrypted and intended only for a trusted private network. There is no remote-input forwarding or driver installation by the host.
