{ config, lib, ... }:
let
  cfg = config.services.catalyrst;
in
lib.mkIf cfg.enable {
  security.apparmor.policies.catalyrst-live = {
    state = "complain";
    profile = ''
      abi <abi/4.0>,
      include <tunables/global>

      profile catalyrst-live /nix/store/*-catalyrst-*/bin/catalyrst-live flags=(complain) {
        include <abstractions/base>
        include <abstractions/nameservice>
        include <abstractions/openssl>
        include <abstractions/ssl_certs>

        /nix/store/*-catalyrst-*/bin/catalyrst-live mr,
        /nix/store/** r,
        /nix/store/**.so* mr,

        /etc/ssl/certs/ca-certificates.crt r,
        /etc/resolv.conf r,
        /etc/hosts r,
        /etc/nsswitch.conf r,
        /etc/services r,

        # Content storage -- own sync area is RW; legacy ref area is read-only
        ${cfg.stateDir}/content_rust/ rw,
        ${cfg.stateDir}/content_rust/** rwk,
        ${cfg.stateDir}/content/ r,
        ${cfg.stateDir}/content/** r,

        /run/postgresql/ r,
        /run/postgresql/.s.PGSQL.${toString cfg.pgPort} rw,

        @{PROC}/sys/kernel/random/uuid r,
        @{PROC}/sys/kernel/random/boot_id r,
        @{PROC}/sys/kernel/version r,
        @{PROC}/cpuinfo r,
        @{PROC}/meminfo r,
        @{PROC}/stat r,
        @{PROC}/loadavg r,
        @{PROC}/uptime r,
        @{PROC}/[0-9]*/maps r,
        @{PROC}/[0-9]*/status r,
        @{PROC}/[0-9]*/cmdline r,
        @{PROC}/[0-9]*/limits r,
        @{PROC}/[0-9]*/comm r,
        @{PROC}/[0-9]*/task/ r,
        @{PROC}/[0-9]*/task/** r,

        /sys/devices/system/cpu/ r,
        /sys/devices/system/cpu/** r,
        /sys/devices/system/node/ r,
        /sys/devices/system/node/** r,
        /sys/fs/cgroup/ r,
        /sys/fs/cgroup/** r,

        # Network -- outbound TCP for sync; loopback TCP (listener bound by
        # systemd); netlink for DNS / interface enumeration.
        network inet stream,
        network inet6 stream,
        network unix stream,
        network unix dgram,
        network netlink raw,

        deny capability,
        deny ptrace,
        deny dbus,
        deny mount,
        deny remount,
        deny umount,
        deny pivot_root,

        signal (receive) peer=unconfined,
        signal (send,receive) peer=catalyrst-live,
      }
    '';
  };

  security.apparmor.policies.catalyrst-pulse = {
    state = "complain";
    profile = ''
      abi <abi/4.0>,
      include <tunables/global>

      profile catalyrst-pulse /nix/store/*-catalyrst-pulse-*/bin/catalyrst-pulse flags=(complain) {
        include <abstractions/base>
        include <abstractions/nameservice>
        include <abstractions/openssl>
        include <abstractions/ssl_certs>

        /nix/store/*-catalyrst-pulse-*/bin/catalyrst-pulse mr,
        /nix/store/** r,
        /nix/store/**.so* mr,

        /etc/ssl/certs/ca-certificates.crt r,
        /etc/resolv.conf r,
        /etc/hosts r,
        /etc/nsswitch.conf r,
        /etc/services r,

        @{PROC}/sys/kernel/random/uuid r,
        @{PROC}/sys/kernel/random/boot_id r,
        @{PROC}/cpuinfo r,
        @{PROC}/meminfo r,
        @{PROC}/stat r,
        @{PROC}/[0-9]*/maps r,
        @{PROC}/[0-9]*/status r,
        @{PROC}/[0-9]*/cmdline r,
        @{PROC}/[0-9]*/limits r,
        @{PROC}/[0-9]*/comm r,
        @{PROC}/[0-9]*/task/ r,
        @{PROC}/[0-9]*/task/** r,

        /sys/devices/system/cpu/ r,
        /sys/devices/system/cpu/** r,
        /sys/devices/system/node/ r,
        /sys/devices/system/node/** r,
        /sys/fs/cgroup/ r,
        /sys/fs/cgroup/** r,

        # Network -- public UDP (ENet game, port 7777); netlink for DNS /
        # interface enumeration.
        network inet dgram,
        network inet6 dgram,
        network unix stream,
        network unix dgram,
        network netlink raw,

        deny capability,
        deny ptrace,
        deny dbus,
        deny mount,
        deny remount,
        deny umount,
        deny pivot_root,

        signal (receive) peer=unconfined,
        signal (send,receive) peer=catalyrst-pulse,
      }
    '';
  };
}
