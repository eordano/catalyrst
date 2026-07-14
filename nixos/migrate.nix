{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  d = import ./helpers.nix cfg;
  # contentPackage is the one package option every profile sets (content-node
  # needs nothing else); bundlesPackage is null under content-node, so it
  # can't be the migrations source here even though colmena's single-host
  # config always had both. Falls back to the same inputs.catalyrst.packages
  # path catalyrst-sync.nix uses when a host leaves contentPackage unset.
  contentPkg =
    if cfg.contentPackage != null then
      cfg.contentPackage
    else
      inputs.catalyrst.packages.${pkgs.system}.catalyrst;
  migrations = "${contentPkg}/share/catalyrst-server/migrations";
in
lib.mkIf cfg.enable {
  systemd.services.catalyrst-content-migrate = {
    description = "Apply catalyrst content-DB schema migrations (idempotent)";
    after = [
      "postgresql.service"
      "postgresql-setup.service"
      "postgresql-ownership.service"
    ];
    requires = [
      "postgresql.service"
      "postgresql-setup.service"
    ];
    before = [ "catalyrst-sync.service" ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      User = "catalyrst";
    };
    script = ''
      set -euo pipefail
      PSQL="${pkgs.postgresql_18}/bin/psql${d.pgPortFlag} -v ON_ERROR_STOP=1 -h /run/postgresql -d content"
      # Per-migration ledger. The previous guard short-circuited on a sentinel
      # table created by 0001, so every migration added later was skipped
      # forever on any DB that already had that table. Track each file instead,
      # and apply it with its ledger row in one transaction so a failure
      # part-way leaves nothing recorded.
      $PSQL -c "CREATE TABLE IF NOT EXISTS public.catalyrst_schema_migrations (
          filename   text PRIMARY KEY,
          applied_at timestamptz NOT NULL DEFAULT now()
      );"
      for f in ${migrations}/*.sql; do
        name=$(basename "$f")
        # psql interpolates :'name' for file/stdin input but never for -c, so
        # the ledger statements go in on stdin and keep psql's safe quoting.
        applied=$(echo "select exists(select 1 from public.catalyrst_schema_migrations where filename = :'name');" \
          | $PSQL -v name="$name" -tAf -)
        if [ "$applied" = t ]; then
          echo "skipping $name (already applied)"
          continue
        fi
        echo "applying $name"
        # Adoption re-runs 0001/0002 against an already-populated DB; every
        # statement in the shipped set is IF NOT EXISTS, so that is a no-op.
        echo "INSERT INTO public.catalyrst_schema_migrations (filename) VALUES (:'name');" \
          | $PSQL -v name="$name" --single-transaction -f "$f" -f -
      done
    '';
  };

  systemd.services.catalyrst-sync = {
    after = [ "catalyrst-content-migrate.service" ];
    wants = [ "catalyrst-content-migrate.service" ];
  };
}
