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
  env.CORONA_SOCKET = "${config.devenv.runtime}/corona.sock";
}
