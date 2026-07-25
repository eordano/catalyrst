# catalyrst-bvimposters routes

Port 5154.

| Route | Behavior |
|---|---|
| `GET /ping` | 200, echoes the request path |
| `GET /status` | 200 JSON: `store_bytes`, `store_entries`, `budget_bytes`, `bake_enabled`, `bake_queue`, `bake_inflight`, `quarantine` |
| `GET /imposters/realms/{realm}/{level}/{x},{y}.{crc}.zip` | serve pipeline: store hit, else synchronous CDN read-through, else optional bake enqueue + 404. `{realm}` accepted and ignored. Headers: `content-type: application/zip`, immutable cache-control, `etag: "{crc}"`, `x-bvi-source: store\|cdn` |
| `GET /imposters/realms/{realm}/{level}/{x},{y}.{crc}-spec.json` | debug: spec member extracted from the stored zip; 404 when absent (no read-through, no bake) |

Invalid keys (level > 5, unaligned tile for level, crc 0 or unparseable) always 404
with no side effects.

Binary modes: `catalyrst-bvimposters seed <realm-cache-dir>` imports an extracted
realm cache into the store as crc-keyed stored zips, prints
`imported N skipped N crc0 N incomplete N`, exits.
