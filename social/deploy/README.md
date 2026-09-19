# Self-hosting dcl.social

Build a release from the repository root on Linux with Node.js 26, the pinned Rust toolchain, Python 3 and the native dependencies required by the Cargo workspace:

```sh
bash social/scripts/prepare-release.sh /tmp/dcl-social-release
```

The directory contains `bin/dcl-social-api`, `public/`, deployment examples, a source revision, runtime library requirements and SHA-256 checksums. `CARGO_TARGET_DIR` is supported. This only prepares files; it does not install or restart services.

Copy the release to your installation and point `/opt/dcl-social/current` at it. Review `runtime-libraries.txt` on the destination. The example systemd unit uses a dynamic user and `/var/lib/dcl-social/chat.sqlite`; preserve and back up that database across upgrades. Set `SOCIAL_TELEMETRY_URL` to your own compatible error sink if desired. The binary binds to loopback port 5191.

For a dedicated hostname, customize `nginx-host.conf` and include it inside nginx's `http` block along with `nginx-http.conf`. It assumes a TLS reverse proxy in front of port 5080. Configure HTTPS and DNS for your hostname. To serve at `/chat/` on an existing site, include `nginx-path.conf` inside that site's server block and `nginx-http.conf` inside `http`. The frontend uses relative asset/API URLs, so the same build supports both layouts.

Validate the local deployment templates before installation:

```sh
SOCIAL_TEST_BINARY="$PWD/target/release/dcl-social-api" \
  bash social/scripts/check-deploy.sh
```

This requires nginx on `PATH` (or `NGINX_BIN`), curl, Python 3 and `systemd-analyze`. It starts disposable loopback processes, verifies configuration and routes, and removes its temporary database afterward. It does not change the running nginx or systemd services.
