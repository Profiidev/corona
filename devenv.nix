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
    vulkan-loader

    # slint lsp
    libinput
    libgbm
    freetype
  ];
in
{
  packages = runtimeDeps ++ buildDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
}
