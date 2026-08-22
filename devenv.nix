{ pkgs, lib, ... }:

let
  runtimeDeps = with pkgs; [
    # slint runtime
    wayland
    fontconfig
    vulkan-loader
    libxkbcommon

    # slint lsp
    libinput
    libgbm
    freetype
  ];
in
{
  packages = runtimeDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
}
