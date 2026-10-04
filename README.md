# Corona

Personal desktop shell in gpui

## Setup

### Hyprland

Add the following layerrules to your config:

```lua
hl.layer_rule({ match = { namespace = "corona_panel" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_notification" }, no_anim = true })
```

## Development

### Gpui-pre update

Update the version in the following files:

- `nix/package.nix`
- `justfile`
- `crates/reqwest/Cargo.toml`

# TODO

script host fn refactor
