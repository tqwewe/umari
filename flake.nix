{
  description = "Umari — event-sourced WASM runtime, server, and CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "wasm32-wasip2" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          pname = "umari-workspace";
          version = "0.1.0";
          strictDeps = true;

          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
            perl
          ];

          buildInputs =
            with pkgs;
            [
              sqlite
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              libiconv
            ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        umari-server = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "umari-server";
            version = "0.1.0";
            cargoExtraArgs = "--package umari-server";
            doCheck = false;
          }
        );

        umari-cli = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "umari-cli";
            version = "0.1.0";
            cargoExtraArgs = "--package umari-cli";
            doCheck = false;
          }
        );
      in
      {
        packages = {
          inherit umari-server umari-cli;
          umari = umari-cli;
          default = umari-server;
        };

        apps = {
          umari-server = flake-utils.lib.mkApp {
            drv = umari-server;
            name = "umari-server";
          };
          umari = flake-utils.lib.mkApp {
            drv = umari-cli;
            name = "umari";
          };
          default = self.apps.${system}.umari-server;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            cargo-make
            git
            protobuf
            sqlite
            pkg-config
            cmake
          ];
        };
      }
    );
}
