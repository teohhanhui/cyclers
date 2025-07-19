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

              (rust-bin.stable."1.87.0".default.override {
                extensions = [ "rust-docs" ];
                targets = [
                  "x86_64-unknown-linux-gnu"
                  "x86_64-pc-windows-gnu"
                  "x86_64-unknown-freebsd"
                  "aarch64-apple-darwin"
                  "aarch64-unknown-linux-gnu"
                ];
              })

              rust-analyzer
            ];

            LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}:${stdenv.cc.targetPrefix}";
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      }
    );
}
