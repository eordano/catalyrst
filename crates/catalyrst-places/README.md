# catalyrst-places

Rust port of `places.decentraland.org`'s REST API (upstream: `decentraland/places`). Reads on the
`places_events` archive; favorites / likes / reports route through the federation write path.
Runs on **:5134**. Routes + parity status: [`ROUTES.md`](./ROUTES.md); stubbed surface:
[`TODO.md`](./TODO.md); stack-wide cutover: [`DEPLOYMENT.md`](../../DEPLOYMENT.md). This file
documents the env this crate reads, with emphasis on the report S3 upload path (audit #18).

## Report uploads - `POST /api/report` (S3 presigned PUT)

Upstream mints an AWS S3 presigned PUT URL (aws-sdk-js v2 `s3.getSignedUrl("putObject", ...)`,
signatureVersion `s3` == SigV2) and the client uploads the report JSON directly to the bucket.
This crate reproduces that wire byte-for-byte (`src/s3.rs`, parity tests pinned to reference
URLs from aws-sdk-js v2).

The presigned-PUT path activates iff all three required vars are set (same names upstream reads);
while they are set there is no way to opt out of the production path.

| var                 | required | meaning |
|---------------------|----------|---------|
| `AWS_ACCESS_KEY`    | yes | IAM access key id |
| `AWS_ACCESS_SECRET` | yes | IAM secret access key |
| `AWS_BUCKET_NAME`   | yes | target bucket |
| `BUCKET_HOSTNAME`   | optional | CDN/proxy host substituted for the S3 host (upstream `url.hostname = BUCKET_HOSTNAME`) |
| `AWS_REGION`        | optional | regional S3 host; default `us-east-1`. SigV2 signs `/bucket/key`, so region only changes the host, never the signature |
| `AWS_ENDPOINT`      | optional | catalyrst-only escape hatch for an S3-compatible store (MinIO/localstack); forces path-style. Upstream never reads it |

Without creds: upstream has no fallback (an unconfigured bucket is a misconfiguration there too).
This crate keeps that fail-closed posture by default while allowing a no-S3 local-dev loop:

| creds set | `PLACES_REPORT_LOCAL_FALLBACK` | result |
|-----------|--------------------------------|--------|
| yes | (ignored) | byte-faithful presigned PUT (production wire) |
| no | unset / `false` | `POST /api/report` -> 503 (fail-closed; logged at ERROR) |
| no | `true` / `1` / `yes` / `on` | same-origin local-upload URL - DEV-ONLY, not the upstream wire, logged at WARN |

The local-upload route is never a silent default: an operator must opt in explicitly with
`PLACES_REPORT_LOCAL_FALLBACK`. Do not set that flag in production - configure `AWS_*` instead.

Local dev (MinIO/localstack) - to exercise the real SigV4/SigV2 wire against a local
S3-compatible store rather than the dev fallback (presigns path-style against the endpoint, so
`POST /api/report` returns a genuine presigned PUT the client can upload to):

```bash
AWS_ACCESS_KEY=minioadmin \
AWS_ACCESS_SECRET=minioadmin \
AWS_BUCKET_NAME=places-reports \
AWS_ENDPOINT=http://localhost:9000 \
  cargo run -p catalyrst-places
```

Other env: see [`src/config.rs`](./src/config.rs) for the full list
(`PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING` and the writer/squid/admin/comms-gatekeeper/events knobs).
