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
