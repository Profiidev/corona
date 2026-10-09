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

    settings = lib.mkOption {
      inherit (toml) type;
      default = { };
      example = {
        language = "de";
        theme.name = "Catppuccin Mocha";
      };
      description = ''
        The greeter's config.toml: `theme` like the shell's `[theme]`, `language`
        over `LANG`.
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
        systemd.tmpfiles.settings."10-corona-greeter"."${home}/.config/corona-greeter/config.toml"."L+" =
          {
            argument = "${toml.generate "corona-greeter.toml" cfg.settings}";
          };

        services.greetd = {
          enable = true;
          # output to the journal, not the VT
          settings.default_session.command = lib.concatStringsSep " " (
            [
              "${pkgs.systemd}/bin/systemd-cat -t corona-greeter"
              "${pkgs.coreutils}/bin/env"
            ]
            ++ lib.mapAttrsToList (name: value: "${name}=${lib.escapeShellArg value}") cfg.environment
            ++ [ "${pkgs.cage}/bin/cage -s -- ${cfg.executable}" ]
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
