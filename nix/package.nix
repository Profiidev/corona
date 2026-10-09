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
  alsa-lib,
  libgbm,
  linux-pam,
  wayland,
  vulkan-loader,
  installShellFiles,
}:

let
  gpuiVersion = "0.3.8";

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

  gpui-pre = vendorCrate "gpui-pre" "sha256-f9FaoOJLrqPgGzH0qMMqZEQu/Up8isYBgzuJWU3Q/Os=";
  # Not on crates.io; patched for ShellRuntime::load_entry. Tag matches Cargo.toml.
  gpui-shell = applyPatches {
    name = "gpui-shell-patched";
    src = fetchFromGitHub {
      owner = "longbridge";
      repo = "gpui-kit";
      tag = "v0.7.1";
      hash = "sha256-NT59GK9+b1WHOPhJz5JF7Ql1P3Wllw6nmE3Zz+lSpfg=";
    };
    patches = lib.filesystem.listFilesRecursive ../patches/gpui-shell;
    patchFlags = [ "-p1" ];
  };
  gpui-pre-linux = vendorCrate "gpui-pre-linux" "sha256-TtGSoMKrW1JbTn6cmgwOLCX8cXXxr6meIkta0phI2vk=";
  gpui-pre-wgpu = vendorCrate "gpui-pre-wgpu" "sha256-/ZwKGDOTUtMO8zYy0ypYQ9mZnSjAYKyFSoWSJr1o5FM=";
in

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "corona";
  version = "0.1.0";

  __structuredAttrs = true;

  src = ./..;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "gpui-base-0.7.1" = "sha256-NT59GK9+b1WHOPhJz5JF7Ql1P3Wllw6nmE3Zz+lSpfg=";
      "llrt_abort-0.9.0-beta" = "sha256-Iu7AAEeCmfPSYPgkR3JXVUi7BYubaAhU5YDApo7LaMQ=";
      "quickjs-jit-0.12.11" = "sha256-4izRB3HxCd9r1ZV/Na6dVmVwEPtX3vFMM1JRp9zu8Go=";
      "quickjs-jit-stdlib-0.12.7" = "sha256-eJuDUwZvmnrlNsGD15ZwhOzz6O4wi9dtc/yUH0nIf0A=";
      "rodio-0.22.2" = "sha256-ihFMFGWPacxEpH0L4wX9P2GPY6rfZSLzMITFJ65qvvk=";
    };
  };

  cargoBuildFlags = [
    "--package"
    "corona"
    "--package"
    "corona_greeter"
  ];

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
    alsa-lib
    libgbm
    linux-pam
  ];

  postPatch = ''
    mkdir -p vendor
    cp -r --no-preserve=mode,ownership ${gpui-pre} vendor/gpui-pre
    cp -r --no-preserve=mode,ownership ${gpui-pre-linux} vendor/gpui-pre-linux
    cp -r --no-preserve=mode,ownership ${gpui-pre-wgpu} vendor/gpui-pre-wgpu
    cp -r --no-preserve=mode,ownership ${gpui-shell}/crates/shell vendor/gpui-shell
  '';

  postFixup = ''
    for bin in corona corona_greeter; do
      patchelf --add-rpath ${
        lib.makeLibraryPath [
          wayland
          vulkan-loader
        ]
      } $out/bin/$bin
    done
  '';

  postInstall = ''
    installShellCompletion --cmd corona \
      --bash <(COMPLETE=bash $out/bin/corona) \
      --zsh  <(COMPLETE=zsh $out/bin/corona) \
      --fish <(COMPLETE=fish $out/bin/corona)
    install -Dm644 assets/corona-settings.svg $out/share/icons/hicolor/scalable/apps/corona-settings.svg
    install -Dm644 assets/corona-settings.desktop $out/share/applications/corona-settings.desktop
    substituteInPlace $out/share/applications/corona-settings.desktop \
      --replace-fail "Exec=corona" "Exec=$out/bin/corona"
  '';

  meta = with lib; {
    description = "A shell for Wayland";
    license = licenses.gpl2;
    maintainers = with maintainers; [ profidev ];
    platforms = platforms.linux;
    mainProgram = "corona";
  };
})
