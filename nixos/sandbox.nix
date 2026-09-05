let
  baseSandbox = {
    NoNewPrivileges = true;
    ProtectSystem = "strict";
    PrivateTmp = true;
    PrivateDevices = true;
    KeyringMode = "private";
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectKernelLogs = true;
    ProtectControlGroups = true;
    ProtectClock = true;
    ProtectHostname = true;
    RestrictAddressFamilies = [
      "AF_UNIX"
      "AF_INET"
      "AF_INET6"
      "AF_NETLINK"
    ];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    LockPersonality = true;
    ProtectProc = "invisible";
    ProcSubset = "pid";
    CapabilityBoundingSet = "";
    AmbientCapabilities = "";
    SystemCallArchitectures = "native";
    SystemCallFilter = [
      "@system-service"
      "~@privileged"
    ];
    UMask = "0077";
    DevicePolicy = "closed";
    RemoveIPC = true;
  };

  commsHardening = baseSandbox // {
    ProtectHome = true;
  };
  noPgSandbox = commsHardening // {
    PrivateUsers = true;
  };
  noJitHardening = noPgSandbox // {
    MemoryDenyWriteExecute = true;
  };

  rootOneshotSandbox = {
    NoNewPrivileges = true;
    ProtectSystem = "strict";
    ProtectHome = true;
    PrivateTmp = true;
    PrivateDevices = true;
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectKernelLogs = true;
    ProtectClock = true;
    ProtectHostname = true;
    ProtectControlGroups = true;
    LockPersonality = true;
    RestrictAddressFamilies = [
      "AF_UNIX"
      "AF_INET"
      "AF_INET6"
      "AF_NETLINK"
    ];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    SystemCallArchitectures = "native";
    # No RestrictFileSystems: these root oneshots exec openssl/awk from the Nix
    # store, whose backing filesystem varies by host (ext4/btrfs/xfs/zfs) and is
    # a network fs in a VM -- an allowlist that omits it denies EXEC and breaks
    # secret generation on first boot. baseSandbox restricts no filesystems for
    # the same reason; ProtectSystem=strict + ReadWritePaths already scope writes.
  };
in
{
  inherit
    baseSandbox
    commsHardening
    noPgSandbox
    noJitHardening
    rootOneshotSandbox
    ;
}
