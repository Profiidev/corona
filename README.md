# Corona

Personal desktop shell in gpui

## Setup

### Hyprland

Add the following layerrules to your config:

```lua
hl.layer_rule({ match = { namespace = "corona_panel" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_notification" }, no_anim = true })
```

Bind the window switcher. It stays open while SUPER is held, Tab moves the selection, Shift reverses it and
releasing SUPER focuses the selected window:

```lua
hl.bind("SUPER + TAB", hl.dsp.exec_cmd("corona ipc switcher"))
hl.bind("SUPER + SHIFT + TAB", hl.dsp.exec_cmd("corona ipc switcher --mode workspace"))
```

## Development

### Gpui-pre update

Update the version in the following files:

- `nix/package.nix`
- `justfile`
- `crates/reqwest/Cargo.toml`
- `crates/capture/Cargo.toml`

# TODO

script host fn refactor
