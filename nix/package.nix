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
  alsa-lib,
  libgbm,
  linux-pam,
  wayland,
  vulkan-loader,
  installShellFiles,
  makeWrapper,
  git,
  dconf,
}:

let
  gpuiVersion = "0.3.11";

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

  gpui-pre = vendorCrate "gpui-pre" "sha256-V1hvZZ8lZ0gyTzqrLgLx/LOCFNMzyc90USPXDufYhcY=";
  gpui-pre-linux = vendorCrate "gpui-pre-linux" "sha256-O8ro+PYCWzEz0TKYzzj4DTTQyHrCBwxo8jMjPEr1yLY=";
  gpui-pre-wgpu = vendorCrate "gpui-pre-wgpu" "sha256-NhoFFN7CgVTskMDuc6po/SX+y0gudmJbCb/F12QIy7k=";
in

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "corona";
  version = "0.1.4";

  __structuredAttrs = true;

  src = ./..;

  cargoLock.lockFile = ../Cargo.lock;

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
    makeWrapper
  ];

  # tests run in tests.yml; the check phase only rebuilds corona for zero tests
  doCheck = false;

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
    wrapProgram $out/bin/corona --suffix PATH : ${
      lib.makeBinPath [
        git
        dconf
      ]
    }
  '';

  postInstall = ''
    installShellCompletion --cmd corona \
      --bash <(COMPLETE=bash $out/bin/corona) \
      --zsh  <(COMPLETE=zsh $out/bin/corona) \
      --fish <(COMPLETE=fish $out/bin/corona)
    # for editors: /run/current-system/sw/share/corona/schema with the NixOS module
    mkdir -p $out/share/corona/schema
    $out/bin/corona schema plugin --output $out/share/corona/schema/plugin.schema.json
    $out/bin/corona schema catalog --output $out/share/corona/schema/catalog.schema.json
    install -Dm644 assets/corona-settings.svg $out/share/icons/hicolor/scalable/apps/corona-settings.svg
    # pkexec is handed current_exe, the binary wrapProgram moves aside in postFixup
    install -Dm644 assets/corona-network.policy $out/share/polkit-1/actions/corona.network.policy
    substituteInPlace $out/share/polkit-1/actions/corona.network.policy \
      --replace-fail "@corona@" "$out/bin/.corona-wrapped"
    install -Dm644 assets/corona-settings.desktop $out/share/applications/corona-settings.desktop
    substituteInPlace $out/share/applications/corona-settings.desktop \
      --replace-fail "Exec=corona" "Exec=$out/bin/corona"
    install -Dm644 assets/corona-url-handler.desktop $out/share/applications/corona-url-handler.desktop
    substituteInPlace $out/share/applications/corona-url-handler.desktop \
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
