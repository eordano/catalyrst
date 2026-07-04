{ lib, stdenv, fetchurl }:

let
  version = "149.3.0";
  shas = {
    x86_64-linux = "sha256-VRk6CADs3K4jGSgCSi9gefAKbB5PRlcLCSe5/hdSaIE=";
  };
  archive = fetchurl {
    name = "librusty_v8-${version}.a.gz";
    url = "https://github.com/denoland/rusty_v8/releases/download/v${version}/librusty_v8_release_${stdenv.hostPlatform.rust.rustcTarget}.a.gz";
    sha256 = shas.${stdenv.hostPlatform.system}
      or (throw "librusty_v8 hash not pinned for ${stdenv.hostPlatform.system}; add it to shas");
    meta.sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
in
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
