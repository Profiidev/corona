{
  description = "Corona";

  nixConfig = {
    extra-substituters = [
      "https://projects.cache.profidev.io"
    ];

    extra-trusted-public-keys = [
      "profidev.cachix.org:tg4xEn64UMdvA5jJYT8omo/CQHk8+spLyeGT2YAku70="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    (flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        pkg = pkgs.callPackage ./nix/package.nix { };
      in
      {
        packages = {
          default = pkg;
          corona = pkg;
        };
      }
    ))
    // {
      nixosModules.default = import ./nix/nixos-module.nix self;
      homeModules.default = import ./nix/home-module.nix self;
      nixosModules.greeter = import ./nix/greeter-module.nix self;

      checks.x86_64-linux.greeter-module = import ./nix/greeter-check.nix {
        inherit self;
        pkgs = nixpkgs.legacyPackages.x86_64-linux;
        inherit (nixpkgs) lib;
      };

      nixosConfigurations.greeter-vm = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          self.nixosModules.greeter
          ./nix/greeter-vm.nix
        ];
      };
    };
}
