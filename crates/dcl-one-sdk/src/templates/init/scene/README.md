# {{TITLE}}

{{DESCRIPTION}}

- **Parcels:** 0,0
- **Base:** 0,0

## Develop

```bash
dcl-one-sdk start
```

node_modules ships with `dcl-one-sdk init` — no npm needed. The npm scripts
(`npm run start`, via `@dcl/sdk-commands`) need a full `npm install` first:
the vendored set omits the npm toolchain to stay small.

## Publish

```bash
npm run deploy
```

Or `dcl-one-sdk deploy --target-content <content-server-url>`.
