{
  pkgs,
  lib,
  config,
  ...
}:

let
  buildDeps = with pkgs; [
    pkg-config
    fontconfig
    libxkbcommon
    libxcb
    pipewire
    alsa-lib
    libgbm
    linux-pam
    rustPlatform.bindgenHook
  ];

  runtimeDeps = with pkgs; [
    wayland
    vulkan-loader
  ];
in
{
  packages = runtimeDeps ++ buildDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
  env.CORONA_SHELL__PLUGIN_DIR = "${config.env.DEVENV_ROOT}/plugins";
}
