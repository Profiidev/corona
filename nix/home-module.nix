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
  tomlFormat = pkgs.formats.toml { };

  generateConfig =
    format: name: value:
    if lib.isString value then
      pkgs.writeText name value
    else if builtins.isPath value || lib.isStorePath value then
      value
    else
      format.generate name value;

  generateToml = generateConfig tomlFormat;
in
{
  options.programs.corona = {
    enable = lib.mkEnableOption "Enable the Corona shell";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${system}.corona;
      description = "The package to use for Corona. If null, the default package will be used.";
    };

    settings = lib.mkOption {
      type = lib.types.attrsOf lib.types.anything;
      default = { };
      description = "Settings for the Corona shell.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    systemd.user.services.corona = {
      Unit = {
        Description = "Corona shell";
        PartOf = [ config.wayland.systemd.target ];
        After = [ config.wayland.systemd.target ];
        X-RestartTriggers = [
          (lib.optional (cfg.settings != { }) "${config.xdg.configFile."corona/config.toml".source}")
          cfg.package
        ];
      };

      Service = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
      };

      Install.WantedBy = [ config.wayland.systemd.target ];
    };

    xdg.configFile."corona/config.toml" = {
      source = generateToml "corona.toml" cfg.settings;
    };
  };
}
