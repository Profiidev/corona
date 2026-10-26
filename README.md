# Corona

Personal desktop shell in gpui

## Setup

### Hyprland

Add the following layerrules to your config:

```lua
hl.layer_rule({ match = { namespace = "corona_panel" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_notification" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_unlock" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_osk" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_switcher" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_launcher" }, no_anim = true })
```

Bind the window switcher. It stays open while SUPER is held, Tab moves the selection, Shift reverses it and
releasing SUPER focuses the selected window:

```lua
hl.bind("SUPER + TAB", hl.dsp.exec_cmd("corona ipc switcher"))
hl.bind("SUPER + SHIFT + TAB", hl.dsp.exec_cmd("corona ipc switcher --mode workspace"))
```

### On-screen keyboard

The keyboard shows when a text field gets focus, by default only in tablet mode. Tablet mode comes
from one of two places, set by `[osk] tablet_source`:

- `hinge`: the hinge angle sensor of a convertible (an IIO device named `hinge`, see
  `grep -l hinge /sys/bus/iio/devices/*/name`). Folded past 200° is tablet mode, back under 160°
  is not. Use this when flipping the screen produces no switch event (`libinput debug-events`).
- `ipc` (default): `corona ipc osk tablet on|off`, for example from Hyprland switch binds. The
  switch's name is listed under `switches` in `hyprctl devices`:

```lua
hl.bind("switch:on:Tablet Mode Switch", hl.dsp.exec_cmd("corona ipc osk tablet on"), { locked = true })
hl.bind("switch:off:Tablet Mode Switch", hl.dsp.exec_cmd("corona ipc osk tablet off"), { locked = true })
```

`corona ipc osk show|hide|toggle` opens it by hand. It takes the seat's input method to see
text fields; with fcitx5 or IBus set `[osk] enabled = false`.

The suggestion wordlists in `assets/wordlists` come from
[hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) under CC-BY-SA 4.0, see
`assets/wordlists/README.md`.

### Hotspot

The hotspot sets its firewall rules through `pkexec corona network ...`. The polkit policy
`assets/corona-network.policy` lets the active session do this without a password. The Nix package
installs it. Elsewhere, install it with the path of the installed binary:

```sh
sed 's|@corona@|/usr/bin/corona|' assets/corona-network.policy \
  | sudo tee /usr/share/polkit-1/actions/corona.network.policy >/dev/null
```

Without it, pkexec asks for the admin password every time.

Corona's iptables rules accept DHCP and DNS from the hotspot's clients. A firewall in its own
nftables table (NixOS with `networking.nftables.enable`, firewalld) still drops them, because an
accept only ends evaluation inside its own table. Open UDP 53 and 67 and TCP 53 on the hotspot's
interface in that firewall. Corona logs a warning when it finds such a table.

On NixOS, set `programs.corona.hotspot.interface` to your wifi interface (for example `"wlp1s0"`).
The module then creates `corona-ap` at boot, keeps NetworkManager's client profiles off it and
opens these ports on it.

### Drive passphrases

`corona ipc drive password-prompt --device <text>` shows a passphrase dialog, prints the
passphrase and exits 0. Cancelling prints nothing and exits 1. udiskie can use it in place of its
own prompt, in `~/.config/udiskie/config.yml`:

```yaml
program_options:
  password_prompt: ["corona", "ipc", "drive", "password-prompt", "--device", "{device_presentation}"]
```

Corona's home-manager module sets this for `services.udiskie` already.

### Printing

The printers panel talks to CUPS. Network printers and AirScan (eSCL) scanners are found over
Avahi (mDNS). Adding queues and pausing printers go through `cups-pk-helper` and need a running
polkit agent. Older scanners without AirScan need sane-backends' `scanimage`.

On NixOS, `programs.corona.recommendedServices.enable` (on by default) turns on CUPS, Avahi,
cups-pk-helper and SANE. Network SANE scanners go in the backend config, for example
`hardware.sane.extraConfig.xerox_mfp = "tcp <ip>";`. Log in again after enabling SANE so its
environment variables are set.

## Development

### Gpui-pre update

Update the version in the following files:

- `nix/package.nix`
- `justfile`
- `crates/reqwest/Cargo.toml`
- `crates/capture/Cargo.toml`
