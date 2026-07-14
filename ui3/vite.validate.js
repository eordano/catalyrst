import { fileURLToPath } from "node:url";

// Resolution for the validation seam, shared by the app and overlay configs so
// the two builds cannot disagree about what a perf build contains.
//
// Performance mode is opt-in via DCL_PERF=1. Two things move together, and they
// have to: aliasing only the implementation would leave every call site still
// importing its schema module, so zod and all 215 schema definitions would stay
// in the bundle and still be constructed at module load. Aliasing the schema
// modules too drops the last import edge, which is what lets the bundler
// actually remove zod.
//
// The stubs are generated (scripts/gen-schema-stubs.mjs) from the real modules'
// exports, so a schema added tomorrow cannot be missing from the perf build --
// a missing export there would be a build error in perf mode only, which is the
// failure nobody would notice until release.

const here = (p) => fileURLToPath(new URL(p, import.meta.url));

export const PERF = process.env.DCL_PERF === "1";

/**
 * Object form, for configs whose `resolve.alias` is an object (sites').
 *
 * It resolves the implementation only, not the schema stubs: the stub alias is
 * a regex and an object alias cannot hold one. That is not a gap for sites
 * today, because its own 190 schema modules are not aliased either -- stripping
 * them is the extraction job, and until that lands a sites perf build swaps the
 * implementation and keeps the schemas, exactly as ui3's did before its stub.
 */
export function validateAliasObject() {
  // Deliberately ignores PERF. This is sites' resolver, and sites' boundaries
  // are the ephemeral private key, the auth chain, the session bearer token and
  // two in-flight payment stores. DCL_PERF is a bundle-size switch for ui3's
  // overlay; spread into sites it also disarmed all of those, so any build that
  // happened to inherit the variable -- and ui3 ships a build:overlay:perf script
  // that exports it -- would ship sites with validation off at auth and money,
  // with no gate covering it (both perf lanes are ui3-only).
  //
  // There is no version of that trade worth taking: the measured prize is a
  // fraction of one bundle, and the cost is unvalidated input at the highest
  // consequence boundaries in the app.
  return {
    "dcl-validate-impl": here("./src/validate/checked.ts"),
  };
}

const CATALYST_SCHEMAS = [
  "backpack",
  "communities",
  "events",
  "notifications",
  "places",
  "profile",
];

export function validateAlias() {
  const alias = [
    {
      find: "dcl-validate-impl",
      replacement: here(PERF ? "./src/validate/unchecked.ts" : "./src/validate/checked.ts"),
    },
  ];
  if (PERF) {
    alias.push({
      find: /^(.*)\/generated\/bridge-schemas$/,
      replacement: here("./src/generated/bridge-schemas.stub.ts"),
    });
    alias.push({
      find: /^(.*)\/generated\/editor-bus-schemas$/,
      replacement: here("./src/generated/editor-bus-schemas.stub.ts"),
    });
    // Matched on the basename rather than on a directory segment, because this
    // module is imported from several depths and its natural sibling specifier
    // ("../persisted-schemas", from src/data/auth) carries no "/data/" to
    // anchor on. A specifier the alias misses is silent -- the perf build keeps
    // zod and still gets smaller -- so the pattern covers every relative form.
    // The leading `../` requirement keeps it off bare package ids, and the stub
    // it resolves to ends in `.stub.ts`, so it cannot re-match itself.
    alias.push({
      find: /^(?:\.{1,2}\/)+(?:.*\/)?persisted-schemas$/,
      replacement: here("./src/data/persisted-schemas.stub.ts"),
    });
    // Basename-anchored for the same reason: the only importer inside ui3 says
    // "./thirdwebSchema", which carries no directory segment to match on. sites
    // imports the same module as "@ui/data/auth/thirdwebSchema" and is not
    // covered here -- its config uses validateAliasObject(), which cannot hold
    // a regex, so a sites perf build keeps zod either way.
    alias.push({
      find: /^(?:\.{1,2}\/)+(?:.*\/)?thirdwebSchema$/,
      replacement: here("./src/data/auth/thirdwebSchema.stub.ts"),
    });
    // The catalyst readers' wire shapes. Anchored on the `schemas/` segment,
    // which is the only one in the tree, so every relative depth a reader or a
    // test reaches them from ("./schemas/places", "../data/catalyst/schemas/
    // places") is covered. These carry the bulk of the definitions, and until
    // they were split out of their readers the readers could not be aliased at
    // all -- a module that mixes schemas with fetches cannot be replaced by a
    // stub.
    //
    // Unlike the modules above, these are parsed directly rather than handed to
    // `check`, and several of them drop malformed rows. Their stubs accept
    // everything, so in perf mode those rows reach the view mappers instead --
    // the reason the stubs are shims rather than nulls, and the reason this mode
    // is only safe against a service whose payloads are trusted.
    for (const name of CATALYST_SCHEMAS) {
      alias.push({
        find: new RegExp(`^(?:\\.{1,2}/)+(?:.*/)?schemas/${name}$`),
        replacement: here(`./src/data/catalyst/schemas/${name}.stub.ts`),
      });
    }
  }
  return alias;
}
