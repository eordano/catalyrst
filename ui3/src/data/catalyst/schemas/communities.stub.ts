// GENERATED from src/data/catalyst/schemas/communities.ts by catalyrst/ui3/scripts/gen-schema-stubs.mts. Do not edit.
//
// Performance-mode stand-in: the perf build aliases the real module here so
// zod leaves the bundle. Every export is the same always-accepting shim, so a
// call site that parses its schema directly keeps working and one that hands it
// to `check` never looks at it -- see src/validate/unchecked.ts.
//
// Accepting everything is the trade the mode makes, and it is a real one. The
// transforms go with the schemas, so a nullish field stays undefined instead of
// normalizing to null; and a reader that used validation to DROP a bad row now
// hands that row to its view mapper, which can throw on a field the row does not
// have. Performance mode trusts the wire -- turn it on only where that holds.

const accept = {
  parse: (value: unknown) => value,
  safeParse: (value: unknown) => ({ success: true as const, data: value }),
} as never;

export const CommunityMemberSchema = accept;
export const CommunityEventSchema = accept;
export const CommunityPostSchema = accept;
export const CommunityPlaceSchema = accept;
export const CommunitySchema = accept;
