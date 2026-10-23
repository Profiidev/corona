# Corona

Personal desktop shell in gpui

## Setup

### Hyprland

Add the following layerrules to your config:

```lua
hl.layer_rule({ match = { namespace = "corona_panel" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_notification" }, no_anim = true })
hl.layer_rule({ match = { namespace = "corona_unlock" }, no_anim = true })
```

Bind the window switcher. It stays open while SUPER is held, Tab moves the selection, Shift reverses it and
releasing SUPER focuses the selected window:

```lua
hl.bind("SUPER + TAB", hl.dsp.exec_cmd("corona ipc switcher"))
hl.bind("SUPER + SHIFT + TAB", hl.dsp.exec_cmd("corona ipc switcher --mode workspace"))
```

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
