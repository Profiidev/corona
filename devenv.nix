{ pkgs, lib, ... }:

let
  buildDeps = with pkgs; [
    # slint build
    libxkbcommon
  ];

  runtimeDeps = with pkgs; [
    # slint runtime
    wayland
    fontconfig
    libglvnd

    # slint lsp
    libinput
    libgbm
  ];
in
{
  packages = runtimeDeps ++ buildDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
}
