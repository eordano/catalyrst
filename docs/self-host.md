# Run your own catalyrst -- NixOS module quickstart

The fastest supported path to a working node is the NixOS module
(`nixosModules.catalyrst`, source in [`nixos/`](../nixos/)). The manual
bundle runbook ([deploy.md](./deploy.md)) remains for non-NixOS hosts, and a
docker-compose distribution is in progress as a third path.

## 0. Provisioning a fresh cloud VPS (skip if you already run NixOS)

The rest of this guide assumes a working NixOS host. Turning a stock cloud
image into one with [`nixos-anywhere`](https://github.com/nix-community/nixos-anywhere)
has three traps that each silently wedge the box before it reaches the
network -- all three verified on a fresh Hetzner cpx42:

- **virtio initrd.** Import `(modulesPath + "/profiles/qemu-guest.nix")` in the
  install config, or stage-1 cannot see the virtio disk and boot hangs before
  login. Most KVM/cloud hosts need this.
- **Boot under either firmware.** New instance types boot UEFI while older ones
  are BIOS; a BIOS-only layout wedges on UEFI and vice-versa. A GPT layout that
  works on both: a 1 MiB `EF02` (BIOS-boot) partition + a 512 MiB `vfat` ESP at
  `/boot` + the root partition, with

  ```nix
  boot.loader.grub = {
    efiSupport = true;
    efiInstallAsRemovable = true;
  };
  ```

- **Host key changes after kexec.** `nixos-anywhere` kexecs into the installer,
  which regenerates the SSH host key, so an `accept-new`/pinned `known_hosts`
  refuses the reconnect mid-run. Give the run explicit host-key handling (e.g.
  `--no-reboot` plus a fresh key fetch, or clear the host's `known_hosts`
  entry between phases).

## 1. Pick a profile

`services.catalyrst.profile` seeds everything else with overridable defaults
(`nixos/options.nix`; both worked examples live in
[`nixos/module-example.nix`](../nixos/module-example.nix)):

| profile | what it is | seeds |
|---|---|---|
| `content-node` | content API + sync only, LAN-friendly | `exposure = "lan"`, `tls = "none"` (plain HTTP :80), no sibling services |
| `full-realm` | a playable realm: content + comms + the bundle tier | comms/explore/create/social/data/socialRpc/explorerApi/worldStorage/profileImages/signatures on |
| `public-gateway` | the default; a full public node with every surface | full-realm + gateway vhosts, governance, presence, telemetry, squid, ab-cdn |

### Will it fit on your box?

`catalyrst-preflight` checks this before the content core starts and refuses
with the actual numbers if not, so you find out at install rather than later.
Its floors, all overridable under `services.catalyrst.preflight`:

| profile | free disk on `stateDir` | RAM | CPUs |
|---|---|---|---|
| `content-node` | 20 GiB | 2 GiB | 2 |
| `full-realm` | 40 GiB | 4 GiB | 2 |
| `public-gateway` | 80 GiB | 8 GiB | 4 |

These are floors to *start*, not a steady-state estimate. What grows is the
blob store, and it grows without bound: a node that mirrors the network's
content sits around 331 GB against a ~10 GB database. A fresh node does not
start anywhere near that -- `SYNC_ENABLED` defaults to false, so it holds only
what is deployed to it. That is why a modest VPS is a fine place to begin and a
poor place to turn sync on.

To boot on a smaller box anyway, either lower the floor
(`services.catalyrst.preflight.minFreeGiB = 15;`) or keep the check advisory
(`services.catalyrst.preflight.strict = false;`), which logs the shortfall and
starts regardless.

## 2. Minimal consumer flake

```nix
nixpkgs.lib.nixosSystem {
  specialArgs = { inherit inputs; };   # the module reads inputs.catalyrst.packages
  modules = [
    inputs.catalyrst.nixosModules.catalyrst
    {
      services.catalyrst = {
        enable = true;
        profile = "public-gateway";
        domain = "example.org";
      };
    }
  ];
}
```

That is the whole configuration -- every `*Package` option defaults null and
falls back to the flake's own build (`contentPackage` -> `packages.catalyrst`,
`bundlesPackage` -> `packages.catalyrst-all`, `squidPackage` ->
`packages.squid`, ...), so a profile is self-contained. Set a package option
only to substitute a private or patched build.

## 3. Required externals, by choice

- **A domain + DNS.** How many records depends on the TLS mode below.
- **TLS** -- `services.catalyrst.tls`:
  - `acme-dns01` (default): a **wildcard** certificate. You manage **one**
    `*.domain` DNS record; supply your provider with `dnsProvider` (`cloudflare`,
    `route53`, ... any lego provider) and its API token in `dnsCredentialsFile`.
    This is the low-toil path if you have DNS-provider API access.
  - `acme-http01`: no DNS token, but the cert carries **one SAN per subdomain**
    (~29 for a full gateway), so you create one A-record each and **every one
    must resolve to this host before the first `nixos-rebuild`**.
  - `none`: LAN and development shapes -- the LAN edge serves plain HTTP on :80.
- **Firewall.** The module opens its own listener ports by default
  (`openFirewall = true`): 80, 443, and the LiveKit/pulse ports when comms is
  on. Set `openFirewall = false` to manage the firewall yourself.
- **eth + polygon RPC endpoints** (only when `subServices.squid` is on -- it
  is, under `public-gateway`): the indexer needs `RPC_ENDPOINT_ETH` and
  `RPC_ENDPOINT_POLYGON` in `squid.env` (S4). These must be **archive** nodes:
  the eth processor reads contract state (e.g. `ownerCutPerMillion`) at
  genesis-era blocks, and a pruned/non-archive endpoint returns empty state --
  the processor then crash-loops on `FunctionResultDecodeError: ... 0x`. A
  general-purpose shared RPC is often *not* archive-grade; use a provider that
  advertises full archive history (or your own archive node). `https://eth.drpc.org`
  + `https://polygon.drpc.org` are keyless and served a full sync at throughput
  in our validation (free tiers can still throttle a multi-hour reindex -- for a
  production node run your own archive or a paid tier). The polygon processor
  additionally requires `SQD_PORTAL_API_KEY` (free key at https://www.sqd.ai):
  it only works against the authenticated SQD portal, and it waits with a
  clear log line -- restarting itself until the key appears in `squid.env` --
  rather than starting without one. To run without the indexer,
  `subServices.squid = false` -- marketplace surfaces then serve honest-empty.
- **TLS first-issue timing.** With `acme-dns01`, the first certificate order
  waits up to 15 minutes for the DNS provider to publish the challenge TXT
  record, and a failed order retries every 15 minutes until the certificate
  is issued; the node serves nixpkgs' self-signed placeholder meanwhile.
  Force an immediate attempt with
  `systemctl restart acme-order-renew-<domain>`.

## 4. Secrets -- what generates itself and what you supply

`services.catalyrst.secretsDir` (default `/var/lib/secrets`) is read via
systemd credentials. Two classes:

**Auto-generated** -- module oneshots mint these on first boot; never write
them by hand: `catalyrst-admin.env` (`SESSION_SECRET`),
`catalyrst-world-storage.env` (`ENCRYPTION_KEY`), `livekit.yaml` +
`livekit-api.env` (LiveKit keys, rotated quarterly by a timer).

**Operator-supplied**:

| file | needed when | contents |
|---|---|---|
| `squid.env` | `subServices.squid` | `RPC_ENDPOINT_ETH` + `RPC_ENDPOINT_POLYGON` (**archive** nodes -- see S3), `DB_SCHEMA=squid_marketplace`, `DB_HOST=/run/postgresql` + `DB_NAME=marketplace_squid` + `DB_USER=squid` (socket peer-auth, no password); `SQD_PORTAL_API_KEY` (+ optional `SQD_PORTAL_URL`) -- required by the polygon processor, which idles with a clear log line until the key is present. Chain IDs and the metrics ports are set by the module. |
| `telemetryAdminTokenFile` (option) | optional | admin token for the telemetry ingest |
| `sites.env` | optional | overrides for the SSR sites tier; the `/server` page persists its own edits to `operator.env` |

The machine-readable map of every service's ports, units, gates and secret
files is [`nixos/facts.nix`](../nixos/facts.nix) -- the module, the `/server`
operator page and the compose generator all derive from it
(`nix eval --json --file nixos/facts.nix`).

## 5. Federation -- off until you decide otherwise

By default (`federation.seedDefault = true`) the module ships a peer seed to
`/etc/catalyrst/federation-peers.toml` and points the worlds server at it.
Every entry has a blank `mtls_root_pem`, and a blank pinned root is a hard
refusal -- so **on a stock node the worlds member refuses the seed and
`/worlds` stays absent (404)** until you fill in at least one peer's real root
certificate. That is deliberate: the seed is a ready-to-fill template, not a
working peer list.

Two ways forward, depending on what you want:

- **Federate.** Follow the header of
  [`nixos/federation-peers.toml`](../nixos/federation-peers.toml) -- copy it,
  paste each peer's root certificate, and point
  `services.catalyrst.federation.peersFile` at the copy. `/worlds` then serves
  and federates.
- **Serve worlds without federating.** Set
  `services.catalyrst.federation.seedDefault = false`. `WORLDS_FED_PEERS_FILE`
  is left unset, so the worlds server serves non-federated content -- a normal
  configuration, not a degraded one.

Gossip is a separate, also-off-by-default layer
([federation.md](./federation.md)):

```nix
services.catalyrst.federation = {
  peerId = "example.org";     # stable public identity (FED_PEER_ID)
  gossip = "nats";            # default "off" = snapshot-pull only
  # natsUrl defaults to the module's own broker (subServices.comms);
  # a remote broker also needs natsRootCa + natsClientCert/natsClientKey.
};
```

`gossip = "nats"` fails at startup when the broker is unreachable -- by
design, instead of silently not publishing.

## 6. Verify

```bash
curl -s https://<domain>/about | jq .content.healthy   # true once sync reaches phase 3
systemctl status catalyrst-sync catalyrst-explore squid-eth
```

The first content bootstrap syncs the full network and takes a while;
`journalctl -u catalyrst-sync -f` shows the phase progression. The `/server`
page (superadmin-gated, `subServices.sites`) shows live per-service health
and collapses services this node does not enable.

## 7. Walking in from a stock client

The desktop Explorer needs no patching to enter a self-hosted realm, but three
things about it are worth knowing before you try.

**Open the protocol handler, not decentraland.org.**
`https://decentraland.org/play/?realm=<your node>` does not work and cannot be
made to: the website forwards `realm` only for realms it whitelists, and
silently drops it otherwise -- so the launcher receives a `position` with no
realm, and the visitor lands at the right coordinates inside Genesis. It looks
like a realm bug because the position survives. Send people a deep link
instead, which the launcher passes through untouched:

```
decentraland://realm=<url-encoded realm url>&position=<x>%2C<y>
```

The node's landing page, the deploy signer, and `dcl-one deploy` all emit this
form now. A world's realm URL is its path on the worlds server
(`https://<domain>/world/<name>`), never the bare ENS name -- a bare name
resolves against Decentraland's worlds server, which is a different world that
merely shares the name.

**A world named `<something>.dcl.eth` collides with the official registry.**
When a realm name ends in `dcl.eth`, the client fetches
`<asset-bundle-registry>/worlds/<realmName>/manifest` from Decentraland's own
infrastructure. If a world of that name is published there, the manifest comes
back non-empty and the client loads scene definitions from the official
registry -- your `scenesUrn` is never read, and visitors see the official copy
of the world while your node serves nothing but `/about`. Since keeping an
official copy of the same name for reach is a sensible thing to do, this is
easy to hit.

catalyrst defends against it by default: a world published to this node
(`world_scenes.deployer` is not the zero address) advertises its realm name
with the ENS suffix stripped, so `swissverse.dcl.eth` is served as
`swissverse`, the manifest lookup comes back empty, and the client loads your
`scenesUrn`. Mirrored worlds keep their ENS name, because for those the
official copy is the point.

Two knobs:

| setting | where | effect |
|---|---|---|
| `WORLDS_REALM_NAME_STRIP_ENS=0` | worlds env | turn the default off node-wide |
| `realm_name_override` | `PUT /world/<name>/settings` | force one world's realm name either way; survives republishing |

The override exists because `worldConfiguration.name` lives in the scene
entity, so every deploy restores it -- editing the entity is not a fix that
lasts.

A side effect worth expecting: with a non-`dcl.eth` realm name the manifest is
empty, so the client generates no auto-terrain around your scenes. Skyboxes
that meet the ground stop showing a seam.

Avoid naming a realm `main`, `shiva`, `hela`, `heimdallr`, `baldr`, `artemis`,
`loki`, `dg`, `hephaestus`, `unicorn`, `marvel` or `nftworld` for the same
class of reason: the client treats those as Genesis realm names and fetches
Decentraland's Genesis manifest for them.

**Comms failure blocks entry; it does not degrade it.** See
[deploy.md](./deploy.md#comms-and-entry) -- the short version is that the node
now detects an unreachable SFU and advertises `offline:offline` so people can
still get in.

## 8. A stock client will not enter my realm

Diagnose in this order. Each step separates two causes that produce the same
symptom -- a client that sits at the loading screen or drops into the wrong
world.

1. **Did the client even receive the realm?** In `Player.log`, the `Arg N:`
   lines dump the arguments the client launched with. `realm` absent while
   `position` is present means the link stripped it before the launcher ran:
   you used a `decentraland.org/play` URL. Use the deep link.

2. **Whose content is rendering?** Filter your origin's access log by the
   `UnityPlayer` user agent. `/about` fetched with no `/contents/*` requests
   following means the client got your realm and then loaded the scenes from
   somewhere else -- the ENS-name collision above.

3. **Which entity is it loading?** Compare the hash in `Player.log`'s
   `Loading scene` line against (a) your node's `scenesUrn` in
   `/world/<name>/about` and (b) `https://worlds-content-server.decentraland.org/world/<name>/scenes`.
   Whichever matches is the server actually serving your visitors.

4. **Does it fetch content and still not enter?** That is comms, not content.
   Check the operator console: a red `COMMS DOWN` banner names the endpoint and
   how long it has been silent.
