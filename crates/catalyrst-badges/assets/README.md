# Badge asset store

Every PNG here mirrors the public upstream badge CDN
(`assets-cdn.decentraland.org`) so each asset URL the API advertises serves
real art through the `/assets` ServeDir route. Static provenance, not a live
dependency: files are fetched once and committed.

Upstream id mapping: `decentraland_citizen` <- `dclcitizen`,
`walkabout` <- `walkabout_wanderer`. Tiered badges' badge-level art is the
upstream starter-tier art (upstream serves no badge-level paths for tiered
badges). Upstream serves no 2d `hrm`/`baseColor` for any badge, so the two
`open_for_business/2d/{hrm,baseColor}.png` paths stay unbacked -- matching
prod. `tests/upstream_parity.rs::advertised_assets_are_backed_by_real_png_files`
pins the two-way set equality between advertised paths and files on disk.
