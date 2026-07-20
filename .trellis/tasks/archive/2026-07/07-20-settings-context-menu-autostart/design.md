# Settings panel, context menu, and autostart design

## Boundaries

The existing `DesktopController` remains the only owner of accepted desktop
state. The new cross-layer flow is:

```text
SettingsView or native menu
  -> typed semantic action / menu ID
  -> Tauri desktop command / dispatcher
  -> DesktopController transaction
  -> native autostart + preference file + shared menu
  -> desktop://preferences-updated
  -> validated useDesktopPreferences projection in both renderers
```

React never imports the autostart plugin. Rust registers the official plugin
and uses its manager extension directly, so no autostart renderer permission or
JavaScript dependency is needed.

## Preference contract

Extend the Rust and TypeScript `DesktopPreferences` shapes with
`launchAtLogin: boolean`, defaulting to `false` through the existing serde
default/renderer constant paths. The field is included in the single preference
JSON, command responses, menu synchronization, and full-state event.

At controller construction, read the native autostart registration. A
successful read replaces the loaded JSON field so UI and menu reflect reality.
If the read itself fails, retain the normalized persisted value and log a
warning so desktop startup and tray recovery remain available.

`DesktopOption::LaunchAtLogin` reuses the existing serialized option command
and transaction. Native application calls plugin `enable`/`disable`, then
verifies `is_enabled` equals the requested value. The established transaction
order remains apply -> persist -> menu sync -> in-memory commit -> complete
event; error paths restore the previous native value and preference/menu state
best-effort.

## Native menu

Add `desktop.launch-at-login` and one `CheckMenuItem` labeled `开机自启` to
`DesktopMenu`. It sits with the visibility/system controls before opacity and
source. `DesktopMenu::sync` derives its checked state only from the complete
preference. Both tray and taskbar context-menu paths continue to reference the
same `Menu` instance, so no duplicate menu construction is introduced.

## Renderer settings view

Add one presentational `SettingsView` under `src/components/`. `App` owns an
explicit compact/detail/settings view state, the originating view for settings,
resize errors, and semantic preference callbacks. `useDesktopPreferences`
remains responsible for typed command responses and live event hydration.

The settings view replaces compact/detail content in the existing main WebView
and uses the existing expanded native bounds. It contains:

- source segment: `主站` / `分布式`;
- window toggles: always-on-top, click-through, position lock, taskbar window,
  and main window;
- opacity segment: `100 / 90 / 80 / 70 / 60`;
- system toggle: start at login.

Opening from compact stores `false`, expands natively, then mounts settings.
Opening from detail stores `true` and mounts without resizing. Back/Escape
first completes any required native resize, then restores the originating
view. A rejected resize leaves the currently coherent view mounted.

The existing `desktop://main-expanded` event continues to synchronize native
geometry. Add a narrow `desktop://show-main-details` intent event emitted only
by `show_main_details`; `App` uses it to leave settings and select detail when
the tray/taskbar companion explicitly requests details. Opening settings does
not emit this intent, avoiding ambiguity between an expanded settings surface
and an expanded radar detail surface.

The source control needs a typed async `set_desktop_radar_source` command that
delegates to the existing serialized `switch_radar_source` transaction. It
must not duplicate source persistence or refresh logic. Other toggles reuse
`set_desktop_option`; opacity reuses `set_desktop_opacity`.

## Failure and compatibility behavior

- Pending settings controls are disabled until their native command settles.
- Command rejection retains event-owned preferences; the settings error is a
  bounded status and does not optimistically select the failed value.
- Enabling click-through or hiding the main window can make settings
  inaccessible by design; the unchanged shared tray menu is always the
  recovery path.
- Taskbar-companion context input is guaranteed only while click-through is
  disabled. This preserves the existing meaning of click-through instead of
  introducing a second partially interactive overlay.
- Autostart uses the normal executable without special arguments. Existing
  startup visibility restoration therefore remains authoritative.
- macOS uses the plugin's default LaunchAgent mechanism. Windows uses the
  plugin's native login registration. Linux remains rejected by the existing
  desktop platform guard.

## Rollback

The code change can be rolled back by removing the new preference field/menu
item/settings view/plugin registration. A legacy file containing the extra
camel-case field remains harmless because serde ignores unknown fields. Native
registrations created while the feature was enabled should be disabled through
the plugin before shipping such a rollback.
