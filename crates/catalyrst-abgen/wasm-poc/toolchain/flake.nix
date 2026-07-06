{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      rust = pkgs.rust-bin.stable."1.97.0".default.override {
        targets = [ "wasm32-unknown-unknown" ];
      };
      wasi = pkgs.pkgsCross.wasi32;
      wasiCC = wasi.stdenv.cc;
    in {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = [
          rust pkgs.git pkgs.binaryen pkgs.python3 pkgs.cmake
          pkgs.nodejs pkgs.draco
        ];
        env = {
          CC_wasm32_unknown_unknown = "${wasiCC}/bin/wasm32-unknown-wasi-cc";
          CXX_wasm32_unknown_unknown = "${wasiCC}/bin/wasm32-unknown-wasi-c++";
          AR_wasm32_unknown_unknown = "${wasiCC}/bin/wasm32-unknown-wasi-ar";
          CFLAGS_wasm32_unknown_unknown = "--target=wasm32-unknown-wasi";
          CXXFLAGS_wasm32_unknown_unknown = "--target=wasm32-unknown-wasi";
          WASI_LIBC_LIB = "${wasiCC.libc}/lib";
          WASI_LIBCXX_LIB = "${wasi.llvmPackages.libcxx}/lib";
        };
      };
    };
}
