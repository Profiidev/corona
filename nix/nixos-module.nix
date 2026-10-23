self:
{
  config,
  lib,
  pkgs,
  utils,
  ...
}:

let
  inherit (pkgs.stdenv.hostPlatform) system;
  cfg = config.programs.corona;
  # corona's own name for the interface it runs a hotspot on beside a wifi connection
  ap = "corona-ap";
  radioDevice = "sys-subsystem-net-devices-${utils.escapeSystemdPath cfg.hotspot.interface}.device";
in
{
  options.programs.corona = {
    enable = lib.mkEnableOption "Enable the Corona shell";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${system}.corona;
      description = "The package to use for Corona. If null, the default package will be used.";
    };

    systemd = {
      target = lib.mkOption {
        type = lib.types.str;
        default = "graphical-session.target";
        description = "The systemd target to start Corona on.";
      };
    };

    hotspot.interface = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "wlp1s0";
      description = ''
        The wifi interface whose radio gets Corona's hotspot interface (corona-ap) at boot, so a
        hotspot runs beside its connection without dropping it while the interface is added.
        Also keeps NetworkManager from connecting client profiles on corona-ap and opens DHCP and
        DNS for the hotspot's clients in the firewall. Only for radios that run a station and an
        access point at once.
      '';
    };

    recommendedServices.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to enable recommended services for Corona.";
    };
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      {
        environment.systemPackages = [ cfg.package ];
        environment.pathsToLink = [ "/share/corona" ];

        security.pam.services.corona.fprintAuth = lib.mkDefault false;

        systemd.user.services.corona = {
          description = "Corona shell";
          partOf = [ cfg.systemd.target ];
          after = [ cfg.systemd.target ];
          wantedBy = [ cfg.systemd.target ];
          restartTriggers = [ cfg.package ];

          serviceConfig = {
            ExecStart = "${lib.getExe cfg.package} shell";
            Restart = "on-failure";
          };
        };
      }

      (lib.mkIf (cfg.hotspot.interface != null) {
        systemd.services.corona-ap = {
          description = "Corona hotspot interface";
          # pulled in by the radio: a radio missing at boot would hold up NetworkManager otherwise
          wantedBy = [ radioDevice ];
          before = [ "NetworkManager.service" ];
          bindsTo = [ radioDevice ];
          after = [ radioDevice ];
          path = [
            pkgs.iw
            pkgs.iproute2
          ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          # never removed: the interface going away restarts wpa_supplicant on some setups
          script = ''
            iw dev ${ap} info >/dev/null 2>&1 && exit 0
            radio=$(cat /sys/class/net/${cfg.hotspot.interface}/phy80211/macaddress)
            # corona's ap_mac: locally administered, distinct from the radio's
            first=$(printf '%02x' $(( 0x''${radio%%:*} | 2 )))
            mac=$first:''${radio#*:}
            if [ "$mac" = "$radio" ]; then
              mac=''${mac%:*}:$(printf '%02x' $(( 0x''${radio##*:} ^ 1 )))
            fi
            iw dev ${cfg.hotspot.interface} interface add ${ap} type __ap
            ip link set ${ap} address "$mac"
          '';
        };

        # no client profile on it from boot on, before corona runs
        networking.networkmanager.settings."device-${ap}" = {
          match-device = "interface-name:${ap}";
          allowed-connections = "id:Corona Hotspot";
        };

        # an accept in another nftables table cannot override this firewall's drop; corona's
        # rejects still keep the clients off everything else
        networking.firewall.interfaces.${ap} = {
          allowedUDPPorts = [
            53
            67
          ];
          allowedTCPPorts = [ 53 ];
        };
      })

      (lib.mkIf cfg.recommendedServices.enable {
        networking.networkmanager.enable = lib.mkDefault true;
        hardware.bluetooth.enable = lib.mkDefault true;
        services.upower.enable = lib.mkDefault true;
        # printers panel: CUPS, network printers and eSCL scanners over mDNS, admin actions over D-Bus
        services.printing.enable = lib.mkDefault true;
        services.avahi.enable = lib.mkDefault true;
        services.avahi.nssmdns4 = lib.mkDefault true;
        services.dbus.packages = [ pkgs.cups-pk-helper ];
        # scanimage for scanners without AirScan
        hardware.sane.enable = lib.mkDefault true;
      })
    ]
  );
}
