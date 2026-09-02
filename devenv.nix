{ pkgs, lib, ... }:

let
  buildDeps = with pkgs; [
    pkg-config
    fontconfig
    freetype
    libxkbcommon
    # gpui-component always pulls gpui_platform/x11, so the X11 libs must link
    # even though corona only ever opens Wayland layer-shell surfaces.
    libxcb
  ];

  runtimeDeps = with pkgs; [
    wayland
    libxkbcommon
    fontconfig
    freetype
    vulkan-loader
    libxcb
  ];
in
{
  packages = runtimeDeps ++ buildDeps;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDeps;
}
