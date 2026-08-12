# catalyrst/deploy/systemd — canonical unit templates

Canonical home of the user-mode systemd unit templates for the whole stack
(`umbrella-*.{service,timer}`, `umbrella.target`, `dcl-one-gates.*`,
`dcl-parity-sweep.*`). Moved here from `umbrella/systemd/` on 2026-07-31:
**catalyrst owns HOW the stack runs; umbrella keeps only instance config**
(env values, secrets, data, nginx vhosts).

## How units get installed

Templates are NOT consumed by systemd directly. `umbrella/scripts/render-units.sh`
substitutes `@UMBRELLA_DIR@` / `@REPO_DIR@` / `@STATE_DIR@` / `@BINDIR@` and writes
renders into `umbrella/data/systemd/` (gitignored instance state); with
`INSTALL_USER_UNITS=1` (the `nix run .#install-units` path in the umbrella flake)
it symlinks the renders into `~/.config/systemd/user/` and daemon-reloads.
The render script reads THIS directory first, then `umbrella/systemd/` for the
few instance-owned units that stayed behind (see below). Override the canonical
source with `UNITS_SRC=<dir>`.

Still living in `umbrella/systemd/` (instance-owned, deliberately not moved):

- Anything matching `*squid-api*` / `*archive-sync*` — owned by other
  workstreams; this reorg did not touch them (their owners may relocate them
  independently, e.g. into `umbrella/systemd/attic/`).
- `umbrella-catalyrst-abgen.service.d/override.conf` — pins an
  instance-specific binary under `umbrella/data/bin/`; drop-ins that encode
  instance state stay with the instance.
- Compat symlinks `umbrella/systemd/<name> -> ../../catalyrst/deploy/systemd/<name>`
  for every moved template, so older tooling that globs `umbrella/systemd/*.service`
  (check-ports.py, check-env-reads.py, check-secrets.py, cycle-env-passwords.py)
  keeps working unchanged.

`umbrella-smoke.{service,timer}` (hourly sites smoke sweep) are templated here
like everything else — the old "hand-installed, no template" note in
`umbrella/README.md` history predates the 2026-07-21 templating.

## REBOOT TRAP — boot restores last mode (dev twins vs reboot)

The dev hot-reload stack (`umbrella-dev-*`) consists of **transient
`systemd-run` units** (tmpfs-backed, gone on reboot), while `umbrella.target`
is `WantedBy=default.target` with linger on — so a reboot always brings the
prod units from these templates up first. Since 2026-07-31 the boot then
**restores the operator's last mode** instead of silently staying on prod:

- **Mode marker** `umbrella/data/mode` (gitignored instance state):
  `umbrella/scripts/up-dev.sh` writes `dev` at the moment it stops the prod
  app tier (an aborted switch — e.g. failed debug build — never marks the box
  dev); the flake's `.#up` (and therefore `.#up-prod`) and `.#install-units`
  write `prod` back.
- **`umbrella-mode-restore.service`** (templated here; `Type=oneshot`,
  `After=umbrella.target network-online.target`, `WantedBy=default.target`,
  kept enabled by `nix run .#install-units`) runs
  `umbrella/scripts/mode-restore.sh` at boot: if the marker says `dev` it
  re-runs `up-dev.sh` (which builds BEFORE stopping prod, so a failed restore
  leaves prod serving); any other marker value is a no-op. The script
  **always exits 0** — a broken dev restore logs loudly
  (`journalctl --user -u umbrella-mode-restore`) but never fails the boot or
  takes prod down.
- `umbrella-dev-health` (repo source `umbrella/scripts/umbrella-dev-health`,
  on PATH via `~/.local/bin`) reports the boot outcome: `boot-restore: dev
  twins will be restored at boot` when marker + enabled unit are in place,
  and keeps a loud `REBOOT TRAP:` warning when dev twins are active but the
  restore would not fire (marker unset/`prod`, or the unit not
  installed/enabled) — including in the sandbox fallback path where systemd
  is unreachable (file-level probe of the `default.target.wants` link).

Disable the restore: `rm umbrella/data/mode` (one-shot — next boot stays
prod) or `systemctl --user disable umbrella-mode-restore.service`
(permanent). Manual recovery after a reboot without the mechanism:
`cd ~/one/umbrella && nix run .#up-dev`.

Operators changing these templates must remember renders only refresh when
`render-units.sh` runs (`nix run .#install-units` / `.#up`); editing a
template here does NOT touch the live `umbrella/data/systemd/` renders.
