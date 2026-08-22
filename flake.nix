{
  description = "A personal wayland shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        inherit (pkgs) lib;
        craneLib = crane.mkLib pkgs;

        unfilteredRoot = ./.;
        src = lib.fileset.toSource {
          root = unfilteredRoot;
          fileset = lib.fileset.unions [
            (craneLib.fileset.commonCargoSources unfilteredRoot)
            ./ui
          ];
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          doCheck = false;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            fontconfig
            libxkbcommon
          ];
        };

        corona = craneLib.buildPackage (
          commonArgs
          // {
            cargoArtifacts = craneLib.buildDepsOnly (
              commonArgs // { buildPhaseCargoCommand = "cargoWithProfile build --locked"; }
            );
          }
        );
      in
      {
        packages.default = corona;
      }
    );
}
