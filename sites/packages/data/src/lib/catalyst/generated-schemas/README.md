# Generated zod schemas -- the runtime validators for catalyrst responses

Every `.ts` module here is emitted by `catalyrst/sites/scripts/gen-zod-schemas.mts` from
the ts-rs bindings in `catalyrst/ui3/src/generated/catalyst/<crate>/` (one module per
crate in the script's `EMIT_CRATES` allowlist -- crates with no external
importer are not emitted; extend the allowlist when a new import appears).
Do not hand-edit: the pre-commit zod gate and `npm run
gen:schemas:check` regenerate and diff, so edits that bypass `npm run
gen:schemas` fail the commit.

Each module also carries `_Assert*` types proving mutual assignability between
`z.infer<typeof XSchema>` and the ts-rs type, so `npm run typecheck` statically
verifies the emitted schema matches the wire shape end to end
(Rust DTO -> ts-rs type -> zod schema).

## House rule

**Hand-written zod schemas for catalyrst service responses are banned.**
Validate a catalyrst response with the generated schema for its DTO, then
apply any client-side shaping (nullish -> defaults, renames, derived fields)
in a separate, explicit normalize step AFTER validation:

```ts
import { PlaceRowSchema } from "./generated-schemas/places";

const row = PlaceRowSchema.parse(raw);
const place = normalizePlace(row);
```

Validation truth stays generated; only normalization is hand-written. If a
response has no generated schema yet, add the ts-rs derive to the Rust DTO and
run `npm run gen:types && npm run gen:schemas` -- do not write a zod schema for
it by hand. (`catalyrst/sites/packages/data/src/lib/catalyst/schema.ts` and
`governance/project-update-detail.ts` are the reference conversions.)

## Regenerating

```bash
cd catalyrst/sites
npm run gen:types      # Rust DTOs -> ts-rs bindings (needs cargo, see gen-ts-types.sh)
npm run gen:schemas    # ts-rs bindings -> these zod modules (pure node)
```
