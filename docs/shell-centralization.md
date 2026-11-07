# Centralize UI + IPC into `corona_shell`

## Context

UI (bar widgets, panels, dashboard items, OSDs, overlays), `IpcCommand` impls, launcher rows and settings sections are spread over ~20 backend crates. Backends depend on `surface`, `ipc`, `components`, `launcher`, `settings`, `js`, `clap` and on each other, so they compile serially and cannot be removed cleanly.

Goal: backends become logic + gpui state only. `shell` owns all presentation and IPC. Cross-backend links go through events wired in `shell`.

## Decisions

- `surface` and `ipc` stay as framework crates (traits, registries, server/client). Surface's own panel/theme IPC + CLI stay there.
- `settings`, `osk`, launcher window base keep their own crates, IPC and CLI. Their contributed parts move to shell:
  - settings sections: notifications, capture, idle, osk, launcher
  - launcher rows/sources: pipewire, brightness, bluez, network_manager, notifications, clipboard
- `script`, `js`, `windows` are untouched. `windows` keeps its `capture` dependency (accepted).
- `capture` overlays and IPC move to shell.
- `components` stays the shared building-block crate.
- Backends keep gpui and their `*Ext` traits for state. They drop `surface`, `ipc`, `components`, `launcher`, `settings`, `clap`, and `js`.
- All clap enums are centralized in the `cli` crate. `cli` depends on `shell` for payload types (`shell::ipc`).
- Script integrations (`register_script`) move to `script/src/integrations/<x>.rs`. `script` depends on the backends.
- `Location` type moves to `utils`. Shell inits `location`.
- Wiring lives in `shell/src/wiring/`.
- Execution: one agent, one commit per integration, sequential.

## Target layout

```
crates/shell/src/
  lib.rs  locale_keys.rs  settings.rs
  bar/            mod.rs (register_all), audio, bluetooth, network, power, media,
                  notifications, tray, sysinfo, clock, pages, control_center
  panel/          mod.rs (register_all), audio/, bluetooth/, network/, media/,
                  brightness/, power/, notifications/, sysinfo/, weather/,
                  clipboard, disks/, printers/, lock/, polkit
  dashboard/      quick_actions/, sliders/, cards/
  osd/            power_profile, brightness, launcher_hud
  overlay/        screenshot/, colorpicker/
  ipc/            mod.rs (register_all), one file per integration
  launcher/       audio, brightness, bluetooth, network, notifications, clipboard, snippets
  settings_pages/ notifications, capture, idle, osk, launcher
  session.rs      Action, plan, run (from auth/commands.rs)
  wiring/         mod.rs (wire_all), one file per link   (phase 2)
```

`auth` keeps polkit agent logic only; its lock UI moves to `panel/lock/`.

## Phase 1: move UI + IPC

Order (fewest deps first): bluez, sysinfo, printing, mpris, notifications, clipboard, tray, pipewire, power, brightness, network_manager, udisks, weather, idle, capture, auth.

Per integration, one commit `refactor: move <x> ui and ipc into shell`:

1. Move `ui/`, `commands*`, `launcher*` and settings-section files into the matching `shell/src/<kind>/` files. Move registration calls into each kind's `register_all`.
2. Expose what the moved code needs from the backend (`pub` state/methods, `*Ext`, types). Move tests with the code they test.
3. Delete moved deps from the backend `Cargo.toml`; add them to `shell/Cargo.toml`.
4. Move clap enums to `crates/cli/src/<x>.rs`; `cli` uses `shell::ipc` payloads.
5. Move `register_script` to `script/src/integrations/<x>.rs`.
6. `cargo check --workspace`, `cargo nextest run` for touched crates, commit.

Special cases:

- `Location` moves to `utils` in the `weather` step; weather stops calling `corona_location::init`.
- `auth`: `Action`, `plan`, `Session` orchestration move to `shell/src/session.rs`. The IPC handler in `shell/src/ipc/auth.rs` calls `session::run`. Keep the `plan` test.
- Keep the init order in `shell/src/lib.rs` unchanged (`init_ipc`, `register_variants`, dbus inits).
- Keep every `COMMAND` string unchanged.

## Phase 2: replace cross-backend links with events (not started)

One commit per link; each wiring file gets a test with a fake emit. `wire_all` runs after all inits.

| Link | Hook | Wiring file |
|---|---|---|
| idle → power/auth | `IdleEvent::Fired { name, action }`; idle keeps ScreenOff + Command; `idle.stay_awake` handle passed to `power.stay_awake_while` | `wiring/idle_session.rs` |
| brightness → location | `location` emits `Changed`; brightness exposes a setter | `wiring/night_light.rs` |
| weather → location | event; `Location` already in utils | `wiring/weather_location.rs` |

Links gone after phase 1 (UI/IPC only): network_manager→bluez, power→notifications, udisks→notifications, brightness→power, auth→power/capture/auth_screen. If logic use remains, add an event the same way.

## Verification

- Per commit: `cargo check --workspace`, `cargo nextest run` for touched crates. Run cargo with the shell env as is.
- End of phase 1: `cargo nextest run --workspace`; `cargo tree -p corona_<backend>` shows no `corona_surface/ipc/components/launcher/settings`.
- Compare `COMMAND` strings before and after (grep).
- No VMs or running shell; the user tests manually and sends logs.
