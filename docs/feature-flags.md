# Feature flags, experiments and audience groups

Status: implemented 2026-07-31. Owner crate: `catalyrst-telemetry` (:5150,
public at `/telemetry/` through the deployment's nginx front).

Three layers decide what a given user sees. Highest wins:

| Layer | Source | Scope |
| --- | --- | --- |
| group target | `flag_group_targets` / `experiment_group_targets` | the users matching one audience group |
| global override | `flag_overrides` / `experiment_overrides` | everyone |
| upstream config | `FLAGS_URL` (`explorer.json`) | everyone, the shipped default |

A matching group target beats the global override on purpose: "off for
everyone, on for internal" is the common operator intent, and the reverse
precedence makes it inexpressible. There is no separate kill switch -- a global
`off` with no group target is the kill switch.

## Audience groups

A group matches a user key two ways, checked in this order:

- **explicit membership** -- the key appears in `members`
- **percentage rollout** -- `bucket(group, user_key) < rollout_pct`

`bucket` is FNV-1a over `"<group>:<user_key>"`, computed in Rust
(`handlers/groups.rs`), deliberately not `DefaultHasher` (documented-unstable
across Rust releases) and not Postgres `hashtext` (an internal). Either would
silently re-shuffle every cohort on an upgrade, moving users between arms
mid-experiment. The bucket is stable for a given (group, user) forever.

When a user matches several groups, `priority DESC, name ASC` decides; the
first match wins per flag.

## The user key

`wallet address when signed in, else the anonymous sid`. Both are opaque
strings to this service -- it never validates the shape, so a group can hold
either. Callers pass `?user=<key>`:

```
GET /dash/flags?user=0xabc...             -> merged flags for that user
GET /dash/experiments?key=<exp>&user=...  -> that user's override/variant
GET /dash/group/resolve?user=0xabc...     -> which groups match, and why
```

Omitting `user` yields the global view (no group targeting applied), which is
what an anonymous visitor and the operator dashboard see by default.

## Authorization

`nginx` serves the whole `/telemetry/` prefix publicly, so anything that
mutates operator state -- or reads group member lists, which are wallet
addresses -- lives in `gated_writes` in `lib.rs`, behind
`CATALYRST_TELEMETRY_ADMIN_TOKEN`. **Any new `/dash/*` mutation belongs there,
not in the public router.** Deliberately public, because the browser and the
sites SSR need them unauthenticated:

- `POST /v1/*` -- Segment ingest
- `GET /dash/flags`, `GET /dash/experiments` -- flag/experiment resolution
- the SSR pages themselves

## Product areas

`product_areas` tags a flag or experiment with a display area
(`POST /dash/area {kind,name,area}`). It is presentation only -- it never
affects resolution -- and lives in its own table so a flag can be filed without
being overridden.
