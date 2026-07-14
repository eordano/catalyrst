{ config, lib, pkgs, ... }:

let
  cfg = config.services.catalyrst;
  pf = cfg.preflight;

  # Floors to START a node, not sizing guidance. They exist to catch the one
  # failure an operator cannot diagnose from inside the box -- a machine too
  # small to ever reach a walk-in -- at the moment it is still cheap to move.
  #
  # What actually grows is the blob store, and it is unbounded: this node's
  # content mirror is ~331 GB against a ~10 GB database. A fresh node does NOT
  # start there, because SYNC_ENABLED defaults to false and it then holds only
  # what is deployed to it -- which is why a modest VPS is a reasonable place to
  # begin and a bad place to enable sync.
  defaultFloors = {
    content-node = { diskGiB = 20; ramGiB = 2; cpus = 2; };
    full-realm = { diskGiB = 40; ramGiB = 4; cpus = 2; };
    public-gateway = { diskGiB = 80; ramGiB = 8; cpus = 4; };
  };
  floor = defaultFloors.${cfg.profile} or defaultFloors.public-gateway;

  minDisk = if pf.minFreeGiB != null then pf.minFreeGiB else floor.diskGiB;
  minRam = if pf.minRamGiB != null then pf.minRamGiB else floor.ramGiB;
  minCpus = if pf.minCpus != null then pf.minCpus else floor.cpus;

  script = pkgs.writeShellApplication {
    name = "catalyrst-preflight";
    runtimeInputs = [ pkgs.coreutils pkgs.util-linux ];
    text = ''
      state_dir=${lib.escapeShellArg cfg.stateDir}
      profile=${lib.escapeShellArg cfg.profile}
      strict=${if pf.strict then "1" else "0"}
      problems=0

      note() { printf 'catalyrst-preflight: %s\n' "$1" >&2; }

      # Measure the filesystem that will actually hold the blobs. Walking up to
      # the nearest existing ancestor matters on a first run, when the state dir
      # has not been created yet and df would fail on the path itself.
      probe="$state_dir"
      while [ ! -d "$probe" ] && [ "$probe" != "/" ]; do probe=$(dirname "$probe"); done

      # No -P here: GNU df refuses -P together with --output, and the pairing
      # fails in the worst possible direction -- df writes nothing, the size
      # parses as 0, and a preflight that defaults to strict then refuses to
      # start every node it exists to protect.
      free_mib=$(df --output=avail -BM "$probe" 2>/dev/null | tail -n1 | tr -dc '0-9')
      if [ -z "$free_mib" ]; then
        note "could not read free space for $probe; skipping the disk floor rather than guessing."
        free_mib=""
      fi
      free_gib=$(( ''${free_mib:-0} / 1024 ))
      if [ -n "$free_mib" ] && [ "$free_gib" -lt ${toString minDisk} ]; then
        note "profile '$profile' needs at least ${toString minDisk} GiB free for $state_dir (on $probe), found ''${free_gib} GiB."
        note "  The blob store is what grows, and it is unbounded -- a full content mirror is ~331 GB."
        note "  Raise it, point services.catalyrst.stateDir at a larger filesystem, or override services.catalyrst.preflight.minFreeGiB."
        problems=$((problems + 1))
      fi

      ram_kib=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
      ram_gib=$(( ram_kib / 1024 / 1024 ))
      if [ "$ram_gib" -lt ${toString minRam} ]; then
        note "profile '$profile' needs at least ${toString minRam} GiB RAM, found ''${ram_gib} GiB."
        problems=$((problems + 1))
      fi

      cpus=$(nproc 2>/dev/null || echo 1)
      if [ "$cpus" -lt ${toString minCpus} ]; then
        note "profile '$profile' expects at least ${toString minCpus} CPUs, found $cpus."
        problems=$((problems + 1))
      fi

      if [ "$problems" -eq 0 ]; then
        note "ok -- profile '$profile': ''${free_gib} GiB free, ''${ram_gib} GiB RAM, $cpus CPUs."
        exit 0
      fi

      if [ "$strict" = "1" ]; then
        note "refusing to start. Set services.catalyrst.preflight.strict = false to boot anyway."
        exit 1
      fi
      note "continuing anyway (preflight.strict = false)."
      exit 0
    '';
  };
in
{
  options.services.catalyrst.preflight = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Check the host can actually hold this profile before the stack starts.
        The failure it exists to catch is a box too small to reach a working
        node, which otherwise shows up much later as unexplained failures.
      '';
    };

    strict = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Refuse to start when the host is under the floor. Set false to log the
        shortfall and boot regardless -- appropriate when you know the workload
        is smaller than the profile's default assumption.
      '';
    };

    minFreeGiB = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = ''
        Override the free-space floor for `stateDir`. Null takes the profile
        default (content-node 20, full-realm 40, public-gateway 80 GiB). These
        are floors to start, not a steady-state estimate: the blob store grows
        without bound, and enabling sync will fill any disk you give it.
      '';
    };

    minRamGiB = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = "Override the RAM floor. Null takes the profile default.";
    };

    minCpus = lib.mkOption {
      type = lib.types.nullOr lib.types.int;
      default = null;
      description = "Override the CPU-count floor. Null takes the profile default.";
    };
  };

  config = lib.mkIf (cfg.enable && pf.enable) {
    systemd.services.catalyrst-preflight = {
      description = "catalyrst preflight (host can hold the selected profile)";
      wantedBy = [ "multi-user.target" ];
      before = [ "catalyrst-sync.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = lib.getExe script;
      };
    };

    # Gate the content core on it, so an undersized host is refused at the unit
    # that makes the node a node rather than surfacing later as unexplained
    # failures. `requires` is unconditional because strictness is decided by the
    # script's exit code: a non-strict run reports the shortfall and exits 0.
    systemd.services.catalyrst-sync = {
      after = [ "catalyrst-preflight.service" ];
      requires = [ "catalyrst-preflight.service" ];
    };
  };
}
