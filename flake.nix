{
  description = "A functional and reactive framework";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rust-stable = pkgs.rust-bin.stable.latest.minimal.override {
            extensions = [ "rust-src" "rust-docs" "clippy" ];
        };
      in
      {
        devShells.default =
          with pkgs;
          mkShell rec {
            buildInputs = [
              pkg-config
              openssl
              clang
              lldb

              rust-stable

              # Override existing stable rustfmt in the current environment with the one from nightly.
              (rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal.override {
                extensions = [ "rustfmt" ];
              }))

              rust-analyzer
            ];

            LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}:${stdenv.cc.targetPrefix}";
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      }
    );
}
