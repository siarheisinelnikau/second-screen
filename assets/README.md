# Application identity

Original SecondScreen display-and-tablet artwork, covered by the project MIT license. The SVG is the source reference, PNG supplies the runtime window/taskbar icon, and ICO supplies executable resources at 16, 20, 24, 32, 48, 64, 128 and 256 pixels.

There is no tray mode: closing the host stops sharing. `scripts/inspect-icons.ps1` maps the packaged executable as resource data and renders its icons without executing the application. This proves resource loading, not OS taskbar integration.
