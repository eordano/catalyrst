# Offline V8 archive for catalyrst-scene-state.
#
# The `v8` crate (rusty_v8) that this crate depends on normally *downloads* a
# prebuilt static `librusty_v8_release_<target>.a` from GitHub in its build.rs.
# That download fails inside a Nix sandbox (no network). The supported escape
# hatch — used by nixpkgs' own deno/codex/windmill/etc. packages — is to fetch
# that exact archive *as a fixed-output derivation* (allowed network, pinned by
# hash) and hand its path to the crate via the `RUSTY_V8_ARCHIVE` env var. The
# build then links the prebuilt V8 instead of downloading or compiling it.
#
# Version MUST match the `v8` crate version pinned in Cargo.toml
# (`v8 = "=149.3.0"`). If you bump the crate, bump `version` here and refresh
# `sha256` with:
#
#   nix-prefetch-url \
#     "https://github.com/denoland/rusty_v8/releases/download/v<VER>/librusty_v8_release_x86_64-unknown-linux-gnu.a.gz" \
#   | xargs nix hash to-sri --type sha256
#
# (Per-target hashes live in the `shas` set below; add your target's system
# triple if you build on something other than x86_64/aarch64 linux.)
{ lib, stdenv, fetchurl }:

let
  version = "149.3.0";
  shas = {
    # sha256 of librusty_v8_release_<rustcTarget>.a.gz from the rusty_v8
    # v149.3.0 GitHub release.
    x86_64-linux = "sha256-VRk6CADs3K4jGSgCSi9gefAKbB5PRlcLCSe5/hdSaIE=";
    # aarch64-linux hash intentionally omitted — fill in when building on arm64:
    #   nix-prefetch-url .../librusty_v8_release_aarch64-unknown-linux-gnu.a.gz
    # aarch64-linux = "sha256-...";
  };
  archive = fetchurl {
    name = "librusty_v8-${version}.a.gz";
    url = "https://github.com/denoland/rusty_v8/releases/download/v${version}/librusty_v8_release_${stdenv.hostPlatform.rust.rustcTarget}.a.gz";
    sha256 = shas.${stdenv.hostPlatform.system}
      or (throw "librusty_v8 hash not pinned for ${stdenv.hostPlatform.system}; add it to shas");
    meta.sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
in
# The v8 crate expects RUSTY_V8_ARCHIVE to be the *decompressed* `.a`, so gunzip
# the release asset into a single-file store path.
stdenv.mkDerivation {
  pname = "librusty_v8";
  inherit version;
  dontUnpack = true;
  nativeBuildInputs = [ ];
  buildPhase = ''
    gzip -dc ${archive} > librusty_v8.a
  '';
  installPhase = ''
    cp librusty_v8.a $out
  '';
}
