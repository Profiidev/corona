{
  lib,
  rustPlatform,
  fetchCrate,
  applyPatches,
  pkg-config,
  fontconfig,
  libxkbcommon,
  libxcb,
  pipewire,
  wayland,
  vulkan-loader,
}:

let
  gpuiVersion = "0.3.6";

  vendorCrate =
    pname: hash:
    applyPatches {
      name = "${pname}-${gpuiVersion}-patched";
      src = fetchCrate {
        inherit pname hash;
        version = gpuiVersion;
      };
      patches = lib.filesystem.listFilesRecursive (../patches + "/${pname}");
      patchFlags = [ "-p3" ];
    };

  gpui-pre = vendorCrate "gpui-pre" "sha256-4+VpBtLm1EETfGfmB9VsKrlKBXOBJoQnyr/SqcS3pdA=";
  gpui-pre-linux = vendorCrate "gpui-pre-linux" "sha256-sb3RqlO3TSyIklHkoqzqP2jNbrYCdUyLjhb/nvecCJ4=";
in

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "corona";
  version = "0.1.0";

  __structuredAttrs = true;

  src = ./..;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "gpui-base-0.6.6" = "sha256-WcRgqgn/pD4LfsFb+6kByk1DzRym1G2qNm/D1fAaCYs=";
      "llrt_abort-0.9.0-beta" = "sha256-Iu7AAEeCmfPSYPgkR3JXVUi7BYubaAhU5YDApo7LaMQ=";
      "quickjs-jit-0.12.9" = "sha256-BykXNq9To8LH8xBZ0OekCHUegsUhzGwh8GennxtsFl0=";
      "quickjs-jit-stdlib-0.12.7" = "sha256-eJuDUwZvmnrlNsGD15ZwhOzz6O4wi9dtc/yUH0nIf0A=";
    };
  };

  nativeBuildInputs = [
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = [
    fontconfig
    libxkbcommon
    libxcb
    pipewire
  ];

  postPatch = ''
    mkdir -p vendor
    cp -r --no-preserve=mode,ownership ${gpui-pre} vendor/gpui-pre
    cp -r --no-preserve=mode,ownership ${gpui-pre-linux} vendor/gpui-pre-linux
  '';

  postFixup = ''
    patchelf --add-rpath ${
      lib.makeLibraryPath [
        wayland
        vulkan-loader
      ]
    } $out/bin/corona
  '';
})
