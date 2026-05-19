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

        # Keep .wit files alongside the cargo defaults so wit_bindgen::generate!
        # can find world/types/deps when expanding macros in the umari SDK crate.
        witOrCargo = path: type:
          (pkgs.lib.hasSuffix ".wit" path)
          || (pkgs.lib.hasSuffix "/deps.toml" path)
          || (pkgs.lib.hasSuffix "/deps.lock" path)
          || (craneLib.filterCargoSources path type);

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = witOrCargo;
          name = "source";
        };

        # utoipa-swagger-ui v9.0.2 downloads this zip at build time. Pin it
        # and serve via file:// so the Nix sandbox can build offline.
        swaggerUiZip = pkgs.fetchurl {
          url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip";
          hash = "sha256-SBJE0IEgl7Efuu73n3HZQrFxYX+cn5UU5jrL4T5xzNw=";
        };

        commonArgs = {
          inherit src;
          pname = "umari-workspace";
          version = "0.1.0";
          strictDeps = true;

          # utoipa-swagger-ui's build.rs copies the zip into OUT_DIR and then
          # mutates it; a direct file:// pointer into the Nix store yields a
          # read-only copy. Stage a writable copy in the build tree first.
          preBuild = ''
            cp ${swaggerUiZip} ./swagger-ui.zip
            chmod +w ./swagger-ui.zip
            export SWAGGER_UI_DOWNLOAD_URL="file://$PWD/swagger-ui.zip"
          '';

          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
            perl
            protobuf
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
