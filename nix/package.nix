{
  lib,
  rustPlatform,
  fetchCrate,
  fetchFromGitHub,
  applyPatches,
  pkg-config,
  fontconfig,
  libxkbcommon,
  libxcb,
  pipewire,
  libgbm,
  wayland,
  vulkan-loader,
  installShellFiles,
}:

let
  gpuiVersion = "0.3.7";

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

  gpui-pre = vendorCrate "gpui-pre" "sha256-ZmzcsmgiJUvhfz8ybIKn1PxnyHvalRmL5whrtvZ+XmY=
sha256-X20/pKW1PhzxEb1fMHoapSC/AwfG3JJgsgmZrD4SoK0=";
  # Not on crates.io; patched for ShellRuntime::load_entry. Tag matches Cargo.toml.
  gpui-shell = applyPatches {
    name = "gpui-shell-patched";
    src = fetchFromGitHub {
      owner = "longbridge";
      repo = "gpui-kit";
      tag = "v0.7.0";
      hash = "sha256-Ii3wGy0gLfT2PiSkrqNWPCT2SdSAz5CruC46UrW/55M=";
    };
    patches = lib.filesystem.listFilesRecursive ../patches/gpui-shell;
    patchFlags = [ "-p1" ];
  };
  gpui-pre-linux = vendorCrate "gpui-pre-linux" "sha256-X20/pKW1PhzxEb1fMHoapSC/AwfG3JJgsgmZrD4SoK0=";
  gpui-pre-wgpu = vendorCrate "gpui-pre-wgpu" "sha256-bpYOVy3kygBfh+y2xmPtIQG6R7Nx5tR3dElg6PEOBzs=";
in

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "corona";
  version = "0.1.0";

  __structuredAttrs = true;

  src = ./..;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "gpui-base-0.7.0" = "sha256-Ii3wGy0gLfT2PiSkrqNWPCT2SdSAz5CruC46UrW/55M=";
      "llrt_abort-0.9.0-beta" = "sha256-Iu7AAEeCmfPSYPgkR3JXVUi7BYubaAhU5YDApo7LaMQ=";
      "quickjs-jit-0.12.9" = "sha256-BykXNq9To8LH8xBZ0OekCHUegsUhzGwh8GennxtsFl0=";
      "quickjs-jit-stdlib-0.12.7" = "sha256-eJuDUwZvmnrlNsGD15ZwhOzz6O4wi9dtc/yUH0nIf0A=";
    };
  };

  nativeBuildInputs = [
    pkg-config
    rustPlatform.bindgenHook
    installShellFiles
  ];

  buildInputs = [
    fontconfig
    libxkbcommon
    libxcb
    pipewire
    libgbm
  ];

  postPatch = ''
    mkdir -p vendor
    cp -r --no-preserve=mode,ownership ${gpui-pre} vendor/gpui-pre
    cp -r --no-preserve=mode,ownership ${gpui-pre-linux} vendor/gpui-pre-linux
    cp -r --no-preserve=mode,ownership ${gpui-pre-wgpu} vendor/gpui-pre-wgpu
    cp -r --no-preserve=mode,ownership ${gpui-shell}/crates/shell vendor/gpui-shell
  '';

  postFixup = ''
    patchelf --add-rpath ${
      lib.makeLibraryPath [
        wayland
        vulkan-loader
      ]
    } $out/bin/corona
  '';

  postInstall = ''
    installShellCompletion --cmd corona \
      --bash <(COMPLETE=bash $out/bin/corona) \
      --zsh  <(COMPLETE=zsh $out/bin/corona) \
      --fish <(COMPLETE=fish $out/bin/corona)
  '';
})
