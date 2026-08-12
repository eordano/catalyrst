# Futures - strategic record, 2026-07

Decided 2026-07-31. Records where the deployment story stands, the futures
that were on the table, and the path chosen. Revisit when one-command lands.

## Where deployment stands

The deploy gradient - changes ripen leftmost-first, and everything left of
the bar is private staging infrastructure:

```
lore overlay > umbrella dev > umbrella prod > saturn front | PUBLIC | interconnected.online
```

As of 2026-07-31 `catalyrst/deploy/` is canonical for the deployable surface:
the systemd unit templates (48), the nginx layer (`00-upstreams` /
`01-catalyst` / `02-worlds`), `ports.nix`, and `env-contract.nix`. umbrella
keeps compat shims/symlinks plus instance-specific confs; the render scripts
read `catalyrst/deploy` first, umbrella overriding by basename.

## Candidate futures

| Candidate | What it is | Verdict |
|---|---|---|
| core/overlay split | separate the upstreamable core from the private overlay | **DECIDED - first** |
| one-command realm | a full realm up via `nix run` | **DECIDED - second**; transparent-front ships standalone before it |
| client-in-the-box | the realm ships a playable web client | **DECIDED** - pinned bevy-explorer artifact (abgen precedent), explicitly NOT an in-repo client build |
| Rust chain indexer | replace the Node squid processors with a Rust indexer | after one-command |
| thin-instance umbrella | umbrella shrinks to instance facts (env, secrets, hostnames); generic deploy logic all in `catalyrst/deploy` | direction of travel; largely subsumed by the split |
| full absorption into `catalyrst/instances` | umbrella's instance definitions move in-repo; umbrella stops being a deploy tree | falls out after one-command; no separate push |
| public distribution | replace the catalyst-owner sprawl - upstream self-hosting needs a 59-repo ops group plus a repo that exists only to fan out pipeline triggers | the standing motivation one-command serves |
| federation of small realms | many small self-hosted realms on the catalyrst-fed primitives | enabled by one-command; horizon, not scheduled |
| single-binary realm | every service in one binary | not a separate future - an optional mode of one-command |
| server-only | distribute catalyrst backend-only, no client | **REJECTED** |

## The decided path

1. **Core/overlay split first.**
2. **One-command realm via `nix run` next.** The transparent front ships
   standalone before it (staged by a parallel lane, 2026-07-31).
3. **Client rides as a pinned bevy-explorer artifact** - same consumption
   model as the abgen flake input; never an in-repo client build.
4. **Rust chain indexer after** one-command.
5. **interconnected.online migrates only after one-command lands.**
6. **Full absorption into `catalyrst/instances` falls out** of the above -
   no separate effort.

Invariants: lore never upstreams. Rejected outright: server-only catalyrst.
Single-binary is an optional mode of one-command, not its own track.
