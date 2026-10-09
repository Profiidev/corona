self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  inherit (pkgs.stdenv.hostPlatform) system;
  cfg = config.services.corona-greeter;
  toml = pkgs.formats.toml { };
  user = config.services.greetd.settings.default_session.user;
  home = config.users.users.${user}.home;
  greeter = pkgs.writeShellScript "corona-greeter" ''
    ${lib.concatMapStrings (name: ''
      ${pkgs.wlr-randr}/bin/wlr-randr --output ${lib.escapeShellArg name} --pos ${
        lib.escapeShellArg cfg.outputs.${name}.position
      } || true
    '') (lib.attrNames cfg.outputs)}
    exec ${cfg.executable}
  '';
in
{
  options.services.corona-greeter = {
    enable = lib.mkEnableOption "the Corona greeter, run by greetd under cage";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${system}.corona;
      description = "The package with the corona_greeter binary.";
    };

    executable = lib.mkOption {
      type = lib.types.str;
      default = "${cfg.package}/bin/corona_greeter";
      defaultText = lib.literalExpression ''"''${cfg.package}/bin/corona_greeter"'';
      description = "The greeter binary, like a checkout's debug build while developing.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Extra environment for cage and the greeter.";
    };

    keyboard =
      let
        xkb = config.services.xserver.xkb;
        option =
          name: description:
          lib.mkOption {
            type = lib.types.str;
            default = xkb.${name};
            defaultText = lib.literalExpression "config.services.xserver.xkb.${name}";
            inherit description;
          };
      in
      {
        layout = option "layout" "XKB layouts, comma separated like `us,de`.";
        variant = option "variant" "XKB variants, one per layout.";
        options = option "options" "XKB options, like `grp:alt_shift_toggle` to switch layouts.";
        model = option "model" "XKB model.";
      };

    outputs = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.submodule {
          options.position = lib.mkOption {
            type = lib.types.str;
            example = "1920,0";
            description = "The output's top left in the layout, `x,y`.";
          };
        }
      );
      default = { };
      example = {
        HDMI-A-1.position = "0,0";
        DP-1.position = "1920,0";
      };
      description = ''
        Where each monitor sits, like Hyprland's `monitor` positions. Cage
        otherwise lines them up left to right in the order they connect.
        Unconnected ones are skipped.
      '';
    };

    cursorTheme = lib.mkOption {
      type = lib.types.nullOr (
        lib.types.submodule {
          options = {
            package = lib.mkOption {
              type = lib.types.package;
              description = "The package with the theme under `share/icons`.";
            };
            name = lib.mkOption {
              type = lib.types.str;
              description = "The theme's directory name under `share/icons`.";
            };
            size = lib.mkOption {
              type = lib.types.int;
              default = 24;
              description = "The cursor size.";
            };
          };
        }
      );
      default = null;
      example = lib.literalExpression ''
        {
          package = pkgs.bibata-cursors;
          name = "Bibata-Modern-Classic";
        }
      '';
      description = "The cursor theme for cage and the greeter.";
    };

    profileIcons = lib.mkOption {
      type = lib.types.attrsOf lib.types.path;
      default = { };
      example = lib.literalExpression "{ alice = ./alice.jpeg; }";
      description = "Profile pictures by user name, linked into AccountsService's icons.";
    };

    settings = lib.mkOption {
      inherit (toml) type;
      default = { };
      example = {
        language = "de";
        monitor = "DP-1";
        theme.name = "Catppuccin Mocha";
      };
      description = ''
        The greeter's config.toml: `theme` like the shell's `[theme]`, `language`
        over `LANG`, `monitor` the output showing the login (default: the leftmost).
      '';
    };
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      {
        hardware.graphics.enable = true;

        # greetd's greeter user lives in /var/empty; the greeter keeps the last
        # session in its home
        users.users.${user} = {
          home = lib.mkDefault "/var/lib/${user}";
          createHome = true;
        };
        systemd.tmpfiles.settings."10-corona-greeter"."${home}/.config/corona-greeter/config.toml"."L+" = {
          argument = "${toml.generate "corona-greeter.toml" cfg.settings}";
        };

        systemd.tmpfiles.settings."10-corona-greeter-icons" = lib.mapAttrs' (
          name: icon:
          lib.nameValuePair "/var/lib/AccountsService/icons/${name}" { "L+".argument = "${icon}"; }
        ) cfg.profileIcons;

        services.corona-greeter.environment = lib.mkMerge [
          (lib.mkIf (cfg.cursorTheme != null) {
            XCURSOR_THEME = cfg.cursorTheme.name;
            XCURSOR_SIZE = toString cfg.cursorTheme.size;
            XCURSOR_PATH = "${cfg.cursorTheme.package}/share/icons";
          })
          # cage's keymap; unset ones keep xkbcommon's defaults
          (lib.filterAttrs (_: value: value != "") {
            XKB_DEFAULT_LAYOUT = cfg.keyboard.layout;
            XKB_DEFAULT_VARIANT = cfg.keyboard.variant;
            XKB_DEFAULT_OPTIONS = cfg.keyboard.options;
            XKB_DEFAULT_MODEL = cfg.keyboard.model;
          })
        ];

        services.greetd = {
          enable = true;
          # output to the journal, not the VT
          settings.default_session.command = lib.concatStringsSep " " (
            [
              "${pkgs.systemd}/bin/systemd-cat -t corona-greeter"
              "${pkgs.coreutils}/bin/env"
            ]
            ++ lib.mapAttrsToList (name: value: "${name}=${lib.escapeShellArg value}") cfg.environment
            ++ [ "${pkgs.cage}/bin/cage -s -- ${greeter}" ]
          );
        };
      }

      # GDM-style handoff: Plymouth drops DRM master but keeps its frame on
      # screen, cage takes over, then Plymouth quits. Quitting first shows the
      # black console in between
      (lib.mkIf config.boot.plymouth.enable {
        services.greetd.greeterManagesPlymouth = true;
        systemd.services.greetd = {
          conflicts = [ "plymouth-quit.service" ];
          after = [ "plymouth-quit.service" ];
          serviceConfig = {
            # idle would wait (up to 5s) for plymouth-quit-wait, which only ends when we quit Plymouth
            Type = lib.mkForce "simple";
            ExecStartPre = "-${pkgs.plymouth}/bin/plymouth deactivate";
            # Quit only after cage drew: closing Plymouth's DRM fd turns off a CRTC still showing
            # its framebuffer. ponytail: fixed delay; a greeter-side signal would be exact.
            ExecStartPost = "-${pkgs.bash}/bin/sh -c 'sleep 3; ${pkgs.plymouth}/bin/plymouth quit --retain-splash'";
          };
        };
      })
    ]
  );
}
