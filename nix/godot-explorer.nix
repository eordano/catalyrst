{ lib
, stdenv
, fetchurl
, fetchFromGitHub
, rustPlatform
, unzip
, autoPatchelfHook
, makeWrapper
, protobuf
, pkg-config
, openssl
, alsa-lib
, libGL
, libxkbcommon
, wayland
, vulkan-loader
, xorg
, fontconfig
, freetype
, dbus
, udev
, ffmpeg
, curl
, xvfb
, coreutils
, runtimeShell
, livekitSupport ? false
}:

# A hermetic Linux export of decentraland/godot-explorer.
#
# The engine is a FORK (decentraland/godotengine, "Protocol Squad"), not stock
# Godot, and the project validates the running binary against the exact
# `<version>.stable.gh.<sha>` string -- so the editor and the export templates
# have to come from that fork's publish, pinned by hash, rather than from
# nixpkgs. Everything else is built from source: the gdext library in `lib/`
# compiles here, and the export itself runs headless during the build.
#
# Bumping the engine means bumping godotBuildSha AND all three hashes together;
# they are one artifact set, and mixing an editor with another build's templates
# fails the version check rather than degrading quietly.

let
  godotVersion = "4.6.2";
  godotBuildSha = "6289a3b2b";
  releaseTag = "${godotVersion}.stable.gh.${godotBuildSha}";
  releasesBase = "https://godot-engine-releases.dclexplorer.com/${releaseTag}";

  src = fetchFromGitHub {
    owner = "decentraland";
    repo = "godot-explorer";
    rev = "7f7302805c7b874c5ad3d248f6a0b4b5ef64fb42";
    hash = "sha256-PErvA6shhq1w/9uUIK9wW1KhOqru71kHe/xilT8DhRo=";
  };

  runtimeLibs = [
    # sentry's GDExtension links libcurl; when it is missing the extension fails
    # to load and every SentrySDK reference in the project's GDScript becomes a
    # parse error, which reads like a source problem rather than a link one.
    curl
    libGL
    libxkbcommon
    wayland
    vulkan-loader
    fontconfig
    freetype
    alsa-lib
    dbus
    udev
    openssl
    ffmpeg
    xorg.libX11
    xorg.libXcursor
    xorg.libXext
    xorg.libXi
    xorg.libXinerama
    xorg.libXrandr
    xorg.libXrender
  ];

  # The fork's editor build. Used only during this build, to run the export.
  godotEditor = stdenv.mkDerivation {
    pname = "godot-editor-dcl";
    version = releaseTag;
    src = fetchurl {
      url = "${releasesBase}/editors/godot.${godotVersion}.stable.linux.editor.x86_64.zip";
      hash = "sha256-qplEeXUVCY2XUtWO3/E77txFw8yaS+O1ySMYpOo3KAw=";
    };
    nativeBuildInputs = [ unzip autoPatchelfHook makeWrapper ];
    buildInputs = runtimeLibs;
    sourceRoot = ".";
    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin
      install -Dm755 "$(find . -maxdepth 2 -type f -name 'godot*editor*x86_64' | head -n1)" \
        $out/bin/godot4
      runHook postInstall
    '';
    # Godot dlopens Vulkan and the Wayland/X libs, so autoPatchelf cannot see
    # them from the ELF alone.
    postFixup = ''
      wrapProgram $out/bin/godot4 \
        --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
    '';
    meta.sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };

  # Export templates, laid out where Godot looks for them.
  #
  # The directory is the PLAIN version, `4.6.2.stable` -- not the fork tag. The
  # editor reports `4.6.2.stable.gh.<sha>` from --version but resolves templates
  # under the version without the `.gh.<sha>` suffix, which the export error
  # names explicitly:
  #   No export template found at the expected path:
  #   .../export_templates/4.6.2.stable/linux_release.x86_64
  exportTemplates = stdenv.mkDerivation {
    pname = "godot-export-templates-dcl";
    version = releaseTag;
    srcs = [
      (fetchurl {
        url = "${releasesBase}/compressed-templates/linux_release.x86_64.zip";
        hash = "sha256-xbzFNTcgTBbcvSrQGBY4fm2+GZCn/0xpgHY8RMy5n34=";
      })
      (fetchurl {
        url = "${releasesBase}/compressed-templates/linux_debug.x86_64.zip";
        hash = "sha256-fwU/GOHPpQgXnJcYv23N+G/iP9e7QKmIAS6W7AUsz0g=";
      })
    ];
    nativeBuildInputs = [ unzip ];
    sourceRoot = ".";
    unpackPhase = ''
      runHook preUnpack
      for s in $srcs; do unzip -o -q "$s"; done
      runHook postUnpack
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/${godotVersion}.stable"
      find . -type f -name 'linux_*.x86_64' -exec install -Dm755 {} "$out/${godotVersion}.stable/" \;
      runHook postInstall
    '';
    meta.sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };

  # rusty_v8 downloads a prebuilt V8 static lib from GitHub in its build script,
  # which the sandbox forbids. Its `RUSTY_V8_ARCHIVE` accepts a local path when
  # the value is not an http(s) URL, and `copy_archive` sniffs the gzip magic --
  # so a fetched .a.gz store path is handed over as-is. The alternative,
  # V8_FROM_SOURCE=1, builds V8 with depot_tools and is hours of work per bump.
  librustyV8 = fetchurl {
    url = "https://github.com/denoland/rusty_v8/releases/download/v0.106.0/librusty_v8_release_x86_64-unknown-linux-gnu.a.gz";
    hash = "sha256-jLYl/CJp2Z+Ut6qZlh6u+CtR8KN+ToNTB+72QnVbIKM=";
  };

  # webrtc-sys does the same thing as rusty_v8 -- its build script fetches a
  # prebuilt libwebrtc from a GitHub release. `LK_CUSTOM_WEBRTC` overrides the
  # lookup with a directory holding include/ and lib/, which is what this
  # unpacks to. The tag and triple are what webrtc-sys-build computes for a
  # linux x86_64 release build; they must move together with the crate.
  libwebrtc = stdenv.mkDerivation {
    pname = "libwebrtc-livekit";
    version = "h264-true-prefixed";
    src = fetchurl {
      url = "https://github.com/robtfm/client-sdk-rust/releases/download/h264-true-prefixed/webrtc-linux-x64-release.zip";
      hash = "sha256-BQrHldh8HcPjQMWy39IEAii6NzGjGBjf8+hxEkPKiAI=";
    };
    nativeBuildInputs = [ unzip ];
    sourceRoot = ".";
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      # The archive carries a single top-level directory; lift its contents so
      # $out/include and $out/lib are where LK_CUSTOM_WEBRTC expects them.
      inner="$(find . -maxdepth 2 -type d -name include | head -n1)"
      cp -r "$(dirname "$inner")"/. $out/
      runHook postInstall
    '';
    meta.sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };

  # The SDK component .proto files are not vendored in the repo: xtask fetches
  # @dcl/protocol from npm and copies `package/proto/**` into
  # lib/src/dcl/components/proto/**, which lib/build.rs then reads to generate
  # the component bindings. Pinned to the same tarball the repo pins, so the
  # generated components match the checkout rather than whatever `next` points
  # at today.
  dclProtocol = fetchurl {
    url = "https://registry.npmjs.org/@dcl/protocol/-/protocol-1.0.0-30827383791.commit-0ff6038.tgz";
    hash = "sha256-4t1VLgNC0ahSnomBbldPWJ8dlXTxAxo+fMTZ5ISt08s=";
  };

  # The project's GDScript references SentrySDK/SentryUser unconditionally, so
  # without this addon the export fails at parse time rather than degrading.
  # xtask installs it into godot/addons/sentry; same pin as the repo's.
  sentryAddon = fetchurl {
    url = "https://github.com/getsentry/sentry-godot/releases/download/1.6.0/sentry-godot-1.6.0+4e3e3e5.zip";
    hash = "sha256-qnEh/sbnnpsMRsfrzC9Xqf7qFnyJbF1vMYOSYg5Lueg=";
  };

  # The gdext library the project loads as `libdclgodot.so`. Built from source.
  dclgodotLib = rustPlatform.buildRustPackage {
    pname = "dclgodot";
    version = "1.13.0";
    inherit src;
    sourceRoot = "${src.name}/lib";
    cargoLock = {
      lockFile = "${src}/lib/Cargo.lock";
      allowBuiltinFetchGit = true;
    };
    nativeBuildInputs = [ pkg-config protobuf rustPlatform.bindgenHook ];

    # Wearable and profile resolution funnel through peer_base(); upstream hard-
    # codes it to peer.decentraland.org per DclEnvironment, so a self-hosted
    # realm silently resolves its avatars against Decentraland's catalyst
    # instead of its own. Honour DCL_PEER_BASE when set, which reaches
    # peer_content() and peer_lambdas() with it. Unset keeps upstream behaviour
    # exactly, so this stays a superset rather than a fork of the semantics.
    #
    # --replace-fail so an upstream refactor of this function breaks the build
    # here rather than silently reverting us to the public catalyst.
    postPatch = ''
      # The engine ships a THIRD-PARTY optimized-asset bucket as its default and
      # reaches it for wearables unless a CLI override is passed -- so a node
      # that believes it is self-hosted still calls out to
      # optimized-assets.dclregenesislabs.xyz, and gets ETC2 mobile packs back
      # (`{}-mobile.zip`) that this desktop GL path then decompresses on CPU.
      # Blank the default rather than delete the constant, so the resolver and
      # its --optimized-content-base-url override keep working for an operator
      # who has a bucket of their own to point at.
      substituteInPlace src/content/content_provider.rs --replace-fail \
        'const ASSET_OPTIMIZED_BASE_URL: &str = "https://optimized-assets.dclregenesislabs.xyz/v4";' \
        'const ASSET_OPTIMIZED_BASE_URL: &str = "";'

      # ...and the flag's own help text, which still advertised the bucket we
      # just removed as its default.
      substituteInPlace src/godot_classes/dcl_cli.rs --replace-fail \
        'description: "Override the default optimized-content base URL (default: https://optimized-assets.dclregenesislabs.xyz/v4). Also accepted as deeplink param.".to_string(),' \
        'description: "Optimized-content base URL. Unset by default: assets are processed at runtime from the realm content server. Also accepted as deeplink param.".to_string(),'

      # An empty base must DISABLE the lane, not build "/hash" relative URLs.
      # Clearing it is a state the engine already supports:
      # set_optimized_wearable_base_url("") stores None and the loader falls
      # back to runtime processing, counted separately from the optimized path.
      substituteInPlace src/content/content_provider.rs --replace-fail \
        'optimized_wearable_base_url: Some(format!("{}/", resolved_optimized_base_url())),' \
        'optimized_wearable_base_url: match resolved_optimized_base_url() {
                base if base.is_empty() => None,
                base => Some(format!("{}/", base)),
            },'

      substituteInPlace src/urls/mod.rs --replace-fail \
        'pub fn peer_base() -> String {
    peer_base_for(resolved_env(ServiceGroup::Catalyst))
}' \
        'pub fn peer_base() -> String {
    if let Ok(base) = std::env::var("DCL_PEER_BASE") {
        let base = base.trim_end_matches('"'"'/'"'"');
        if !base.is_empty() {
            return base.to_string();
        }
    }
    peer_base_for(resolved_env(ServiceGroup::Catalyst))
}'
    '';

    # Stage the protos before cargo runs: build.rs reads the directory eagerly
    # and panics if it is absent.
    preBuild = ''
      mkdir -p src/dcl/components
      tar -xzf ${dclProtocol} -C "$TMPDIR" package/proto
      cp -r "$TMPDIR/package/proto" src/dcl/components/

      # build.rs hardcodes ../.bin/protoc/bin/protoc, canonicalizes it, and
      # then OVERWRITES $PROTOC with the result -- so exporting PROTOC is not
      # enough, the binary has to exist at that path. xtask normally downloads
      # it there; nixpkgs' protobuf provides the same tool.
      # unpackPhase preserves the store's read-only mode on the tree above
      # sourceRoot, and build.rs will only look at this relative path.
      chmod -R u+w ..
      mkdir -p ../.bin/protoc/bin
      ln -sf ${protobuf}/bin/protoc ../.bin/protoc/bin/protoc
    '';
    # webrtc-sys links against X11/Xext directly, so the X libs have to be here
    # and not only in the export derivation's runtime closure.
    buildInputs = runtimeLibs;
    PROTOC = "${protobuf}/bin/protoc";
    RUSTY_V8_ARCHIVE = librustyV8;
    LK_CUSTOM_WEBRTC = libwebrtc;
    # A cdylib has no tests worth running here and several touch the network.
    doCheck = false;
    installPhase = ''
      runHook preInstall
      install -Dm755 target/*/release/libdclgodot.so \
        $out/lib/libdclgodot_linux/libdclgodot.so
      runHook postInstall
    '';
  };
in
stdenv.mkDerivation {
  pname = "decentraland-godot-client";
  version = "1.13.0";
  inherit src;

  nativeBuildInputs = [ godotEditor unzip autoPatchelfHook makeWrapper ];
  buildInputs = runtimeLibs;

  buildPhase = ''
    runHook preBuild

    # The .gdextension points at lib/target/libdclgodot_linux/libdclgodot.so
    # relative to the project, so the built library goes exactly there rather
    # than being referenced from the store by an absolute path the export would
    # not follow.
    mkdir -p lib/target/libdclgodot_linux
    cp ${dclgodotLib}/lib/libdclgodot_linux/libdclgodot.so lib/target/libdclgodot_linux/

    # The archive is already rooted at addons/sentry/, and sentry.gdextension
    # refers to res://addons/sentry/bin/... -- so it unpacks straight into the
    # project directory with no path surgery.
    unzip -q -o ${sentryAddon} -d godot/
    test -f godot/addons/sentry/bin/linux/x86_64/libsentry.linux.release.x86_64.so \
      || { echo "sentry addon did not land where sentry.gdextension expects it"; exit 1; }

    # Godot resolves templates and its own config out of HOME/XDG, both of which
    # are unset in the sandbox; without these it writes to / and dies.
    export HOME="$TMPDIR/home"
    export XDG_DATA_HOME="$HOME/.local/share"
    export XDG_CONFIG_HOME="$HOME/.config"
    mkdir -p "$XDG_DATA_HOME/godot"
    ln -s ${exportTemplates} "$XDG_DATA_HOME/godot/export_templates"

    # The editor wrapper carries its own LD_LIBRARY_PATH, but the GDExtensions
    # it dlopens are resolved in this process's environment.
    export LD_LIBRARY_PATH="${lib.makeLibraryPath runtimeLibs}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

    mkdir -p exports

    # Import before export: a cold project has no res://.godot/imported, and an
    # export against one produces a binary whose resources fail at runtime
    # rather than at build time. Godot needs two passes -- the first generates
    # the .import metadata, the second resolves references between the newly
    # imported resources -- and the first legitimately logs missing-resource
    # errors while it is still producing them, so its exit status is not a
    # verdict. The export that follows IS, and it is not swallowed.
    echo ">> godot import pass 1"
    godot4 --headless --path godot --import || true
    echo ">> godot import pass 2"
    godot4 --headless --path godot --import || true

    echo ">> godot export"
    godot4 --headless --path godot \
      --export-release linux ../exports/decentraland.godot.client.x86_64

    test -s exports/decentraland.godot.client.x86_64 \
      || { echo "export produced no binary"; exit 1; }

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/decentraland-godot-client $out/bin
    cp -r exports/. $out/share/decentraland-godot-client/
    chmod +x $out/share/decentraland-godot-client/decentraland.godot.client.x86_64
    makeWrapper $out/share/decentraland-godot-client/decentraland.godot.client.x86_64 \
      $out/bin/decentraland.godot.client.x86_64 \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}

    # Rendering needs a real GL context: --headless selects Godot's dummy
    # rendering server, where async_get_viewport_image() returns null and every
    # render dies on `Parameter "t" is null`. So this variant carries a
    # throwaway X display, and defaults to software GL because the target VPS
    # has no GPU (a box with one can set LIBGL_ALWAYS_SOFTWARE=0).
    #
    # Xvfb is started directly rather than through xvfb-run, because the
    # service sandbox kills xvfb-run's xauth/mcookie helpers with SIGSYS under
    # SystemCallFilter=@system-service -- the X server itself is fine there.
    # A private display that does not listen on TCP has nobody to authenticate,
    # so the cookie those helpers exist to mint buys nothing here.
    #
    # -displayfd lets the server choose a free display and report it back,
    # which is race-free where picking a number and hoping is not. Do NOT add
    # -terminate: godot resets the X server while it imports, and the display
    # would vanish mid-render.
    cat > $out/bin/decentraland-godot-client-xvfb <<EOF
#!${runtimeShell}
set -u
# An X server that already exists is used as-is: Xvfb has no GPU backend, so
# spawning one pins the render to llvmpipe even on a machine with a card. A
# GPU-backed DISPLAY renders the same frames about twice as fast.
if [ -n "\''${DISPLAY:-}" ]; then
  export LIBGL_ALWAYS_SOFTWARE="\''${LIBGL_ALWAYS_SOFTWARE:-0}"
  exec $out/bin/decentraland.godot.client.x86_64 "\$@"
fi
export LIBGL_ALWAYS_SOFTWARE="\''${LIBGL_ALWAYS_SOFTWARE:-1}"
tmp=\$(${coreutils}/bin/mktemp -d)
cleanup() {
  [ -n "\''${xpid:-}" ] && kill "\$xpid" 2>/dev/null
  ${coreutils}/bin/rm -rf "\$tmp"
}
trap cleanup EXIT INT TERM
${xvfb}/bin/Xvfb -displayfd 3 -screen 0 1280x1024x24 -nolisten tcp 3>"\$tmp/disp" &
xpid=\$!
disp=""
i=0
while [ \$i -lt 200 ]; do
  disp=\$(${coreutils}/bin/cat "\$tmp/disp" 2>/dev/null || true)
  [ -n "\$disp" ] && break
  kill -0 "\$xpid" 2>/dev/null || break
  ${coreutils}/bin/sleep 0.1
  i=\$((i + 1))
done
if [ -z "\$disp" ]; then
  echo "decentraland-godot-client-xvfb: Xvfb did not come up" >&2
  exit 1
fi
DISPLAY=":\$disp" $out/bin/decentraland.godot.client.x86_64 "\$@"
EOF
    chmod +x $out/bin/decentraland-godot-client-xvfb
    runHook postInstall
  '';

  meta = with lib; {
    description = "Decentraland godot-explorer, exported headless for Linux";
    longDescription = ''
      The avatar renderer catalyrst-profile-images drives to produce face and
      body thumbnails. Engine is decentraland's Godot fork, pinned by build SHA.
    '';
    platforms = [ "x86_64-linux" ];
    sourceProvenance = [ sourceTypes.fromSource sourceTypes.binaryNativeCode ];
  };
}
