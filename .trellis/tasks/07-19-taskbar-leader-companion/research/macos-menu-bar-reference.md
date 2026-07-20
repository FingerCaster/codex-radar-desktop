# macOS menu-bar reference research

## Source And Copyright

- Local reference: `D:\UGit\NetTool-1.2`
- No repository license file is present. Swift files state copyright 2018 Liu,
  Tao (Toni), all rights reserved.
- Use: product behavior and AppKit architecture evidence only. Do not copy its
  source implementation.

## Relevant Files

- `NeTool/AppDelegate.swift`: creates an `NSStatusItem` with fixed length,
  attaches an `NSMenu`, and installs a custom status view.
- `NeTool/StatusBarView.swift`: fixed-height control, light/dark text drawing,
  click-to-menu behavior, main-thread redraw, and menu highlight lifecycle.
- `NeTool/Info.plist`: `LSUIElement=true`, confirming a menu-bar accessory app
  rather than an ordinary Dock-window presentation.

## Mechanism Observed And Tauri Adaptation

NetTool is a true `NSStatusItem`; it is not a floating `NSWindow`. Tauri 2.11.5
already exposes the corresponding status-item behavior through `TrayIcon`.
`TrayIcon::set_title` supports macOS and can show frequently changing text next
to the icon, while Windows explicitly does not support tray titles.

The Model Radar macOS MVP therefore keeps one native tray/status item and sets
a bounded single-line leader/IQ title. The existing tray menu remains the
interaction and recovery surface. `showTaskbarWindow` toggles the title, not the
icon. A renderer-to-Rust command publishes an already normalized companion
projection whenever radar state changes. No second macOS WebView is created
for this feature.
