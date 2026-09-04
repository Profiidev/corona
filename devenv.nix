{ pkgs, lib, ... }:

let
  buildDeps = with pkgs; [
    pkg-config
    fontconfig
    libxkbcommon
    libxcb
  ];

  runtimeDeps = with pkgs; [
    wayland
    vulkan-loader
  ];
in
{
  packages = runtimeDeps ++ buildDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
}
