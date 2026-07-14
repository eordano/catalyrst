# Real-IP restore only means anything behind Cloudflare's edge -- the
# acme-dns01 wildcard shape whose DNS provider is Cloudflare specifically, not
# every public exposure or every dns01 provider. Key on dnsProvider, not
# exposure. (An operator using Cloudflare DNS without proxying traffic through
# it is over-served here -- a future cloudflareProxied flag would separate the
# two; the reference deployment is both, so this default holds.)
{
  config,
  pkgs,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  inherit (import ./sandbox.nix) rootOneshotSandbox;
  useCloudflare =
    cfg.exposure == "public" && cfg.tls == "acme-dns01" && cfg.dnsProvider == "cloudflare";
in
lib.mkIf (cfg.enable && useCloudflare) {
  services.nginx.commonHttpConfig = ''
    include /var/lib/cloudflare/nginx-real-ip.conf;
    real_ip_header CF-Connecting-IP;
    real_ip_recursive on;
  '';

  systemd.tmpfiles.rules = [
    "d /var/lib/cloudflare         0755 root root -"
  ];

  environment.etc."cf-nginx-real-ip-seed.conf".text = ''
    set_real_ip_from 173.245.48.0/20;
    set_real_ip_from 103.21.244.0/22;
    set_real_ip_from 103.22.200.0/22;
    set_real_ip_from 103.31.4.0/22;
    set_real_ip_from 141.101.64.0/18;
    set_real_ip_from 108.162.192.0/18;
    set_real_ip_from 190.93.240.0/20;
    set_real_ip_from 188.114.96.0/20;
    set_real_ip_from 197.234.240.0/22;
    set_real_ip_from 198.41.128.0/17;
    set_real_ip_from 162.158.0.0/15;
    set_real_ip_from 104.16.0.0/13;
    set_real_ip_from 104.24.0.0/14;
    set_real_ip_from 172.64.0.0/13;
    set_real_ip_from 131.0.72.0/22;
    set_real_ip_from 2400:cb00::/32;
    set_real_ip_from 2606:4700::/32;
    set_real_ip_from 2803:f800::/32;
    set_real_ip_from 2405:b500::/32;
    set_real_ip_from 2405:8100::/32;
    set_real_ip_from 2a06:98c0::/29;
    set_real_ip_from 2c0f:f248::/32;
  '';
  systemd.services.cloudflare-ips-seed = {
    description = "Seed /var/lib/cloudflare/nginx-real-ip.conf on first boot";
    wantedBy = [ "multi-user.target" ];
    before = [ "nginx.service" ];
    serviceConfig = rootOneshotSandbox // {
      Type = "oneshot";
      User = "root";
      ReadWritePaths = [ "/var/lib/cloudflare" ];
    };
    script = ''
      ${pkgs.coreutils}/bin/cp -n /etc/cf-nginx-real-ip-seed.conf /var/lib/cloudflare/nginx-real-ip.conf || true
      ${pkgs.coreutils}/bin/chmod 0644 /var/lib/cloudflare/nginx-real-ip.conf
    '';
  };

  systemd.services.cloudflare-ips-refresh = {
    description = "Refresh Cloudflare edge IP ranges (nginx real-ip include)";
    after = [
      "network-online.target"
      "cloudflare-ips-seed.service"
    ];
    wants = [ "network-online.target" ];
    serviceConfig = rootOneshotSandbox // {
      Type = "oneshot";
      User = "root";
      ReadWritePaths = [
        "/var/lib/cloudflare"
        "/var/lib/node-exporter-textfile"
      ];
    };
    script = ''
      set -euo pipefail
      umask 022
      DIR=/var/lib/cloudflare
      METRIC=/var/lib/node-exporter-textfile/cloudflare_ips_refresh.prom

      v4=$(${pkgs.coreutils}/bin/mktemp "$DIR/.ips-v4.XXXXXX")
      v6=$(${pkgs.coreutils}/bin/mktemp "$DIR/.ips-v6.XXXXXX")
      trap 'rm -f "$v4" "$v6"' EXIT

      if ! ${pkgs.curl}/bin/curl -sf --max-time 30 https://www.cloudflare.com/ips-v4 -o "$v4"; then
        ${pkgs.util-linux}/bin/logger -t cloudflare-ips "fetch v4 failed; keeping previous"
        exit 0
      fi
      if ! ${pkgs.curl}/bin/curl -sf --max-time 30 https://www.cloudflare.com/ips-v6 -o "$v6"; then
        ${pkgs.util-linux}/bin/logger -t cloudflare-ips "fetch v6 failed; keeping previous"
        exit 0
      fi
      if ! ${pkgs.gnugrep}/bin/grep -Eq '^[0-9].*/[0-9]+$' "$v4" \
         || ! ${pkgs.gnugrep}/bin/grep -Eq '^[0-9a-fA-F:].*/[0-9]+$' "$v6"; then
        ${pkgs.util-linux}/bin/logger -t cloudflare-ips "sanity check failed; keeping previous"
        exit 0
      fi

      ngx=$(${pkgs.coreutils}/bin/mktemp "$DIR/.nginx-real-ip.XXXXXX")
      trap 'rm -f "$v4" "$v6" "$ngx"' EXIT
      ${pkgs.gawk}/bin/awk '{printf "set_real_ip_from %s;\n", $0}' "$v4" "$v6" > "$ngx"
      ${pkgs.coreutils}/bin/chmod 0644 "$ngx"
      ${pkgs.coreutils}/bin/mv "$ngx" "$DIR/nginx-real-ip.conf"
      trap - EXIT

      systemctl reload nginx.service || true

      ${pkgs.coreutils}/bin/mkdir -p "$(dirname "$METRIC")"
      printf '# HELP cloudflare_ips_refresh_timestamp_seconds Unix time of last CF IP refresh\n# TYPE cloudflare_ips_refresh_timestamp_seconds gauge\ncloudflare_ips_refresh_timestamp_seconds %d\n' "$(${pkgs.coreutils}/bin/date +%s)" > "$METRIC"
    '';
  };
  systemd.timers.cloudflare-ips-refresh = {
    description = "Daily Cloudflare edge IP refresh";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "daily";
      RandomizedDelaySec = "1h";
      Persistent = true;
    };
  };
}
