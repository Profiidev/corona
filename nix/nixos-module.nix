self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  inherit (pkgs.stdenv.hostPlatform) system;
  cfg = config.programs.corona;
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

      (lib.mkIf cfg.recommendedServices.enable {
        networking.networkmanager.enable = lib.mkDefault true;
        hardware.bluetooth.enable = lib.mkDefault true;
        services.upower.enable = lib.mkDefault true;
      })
    ]
  );
}
