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
      description = ''
        Settings for the Corona shell, written to ~/.config/corona/config.toml.
        See config.example.toml in the corona repository for every setting and
        its default. Changes the shell makes itself (theme mode, the settings
        app) go to ~/.local/state/corona/settings.toml and win over these.
      '';
    };

    toolkits.enable = lib.mkEnableOption ''
      theming GTK and Qt apps from the Corona theme: turns on
      theme.sync_toolkits, makes gtk.css import Corona's colors and points
      qt5ct/qt6ct at its color scheme. Uses adw-gtk3 for GTK 3 apps
    '';
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      (lib.mkIf cfg.toolkits.enable (
        let
          qtct = name: {
            Appearance = {
              custom_palette = true;
              color_scheme_path = "${config.xdg.configHome}/${name}/colors/corona.conf";
            };
          };
        in
        {
          programs.corona.settings.theme.sync_toolkits = lib.mkDefault true;

          gtk = {
            enable = true;
            theme = lib.mkDefault {
              name = "adw-gtk3";
              package = pkgs.adw-gtk3;
            };
            # libadwaita brings its own theme, Corona's colors go over it
            gtk4.theme = lib.mkDefault null;
            # Corona writes corona.css next to it
            gtk3.extraCss = ''@import url("corona.css");'';
            gtk4.extraCss = ''@import url("corona.css");'';
          };
          dconf.enable = true;

          qt = {
            enable = true;
            platformTheme.name = lib.mkDefault "qtct";
            qt5ctSettings = qtct "qt5ct";
            qt6ctSettings = qtct "qt6ct";
          };
        }
      ))
      {
        home.packages = [ cfg.package ];

        systemd.user.services.corona = {
          Unit = {
            Description = "Corona shell";
            PartOf = [ config.wayland.systemd.target ];
            After = [ config.wayland.systemd.target ];
            X-RestartTriggers =
              lib.optional (cfg.settings != { }) "${config.xdg.configFile."corona/config.toml".source}"
              ++ [ "${cfg.package}" ];
          };

          Service = {
            ExecStart = "${lib.getExe cfg.package} shell";
            Restart = "on-failure";
          };

          Install.WantedBy = [ config.wayland.systemd.target ];
        };

        xdg.configFile."corona/config.toml" = {
          source = generateToml "corona.toml" cfg.settings;
        };
      }
    ]
  );
}
