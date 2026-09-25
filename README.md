# Corona

Personal desktop shell in gpui

## Setup

### Hyprland

Add the following layerrule to your config:

```lua
hl.layer_rule({ match = { namespace = "corona_panel" }, no_anim = true })
```

## Development

### Gpui-pre update

Update the version in the following files:

- `nix/package.nix`
- `justfile`

# TODO

script host fn refactor
