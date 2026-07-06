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
      rust = pkgs.rust-bin.nightly."2026-04-15".default.override {
        extensions = [ "rust-src" "llvm-tools" "llvm-bitcode-linker" ];
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = [ rust ];
      };
    };
}
