// What a catalyst reader ACCEPTS, in both build modes.
//
// A perf build aliases the six schema modules to always-accepting stubs. That
// removes checking, and with it every drop a reader was performing through
// `safeParse` -- but the two losses are not symmetric. The default build dropped
// a malformed row and returned the rest; the perf build keeps it and hands it to
// a view mapper, which dereferences a field the row does not have and takes the
// whole read down with it. Perf mode turned a shorter list into no list, which
// is strictly worse than the validation it removed.
//
// So the drop is split in two. A SCHEMA says what is correct, and a perf build
// removes it. A GUARD says what the mapper can use; it lives beside the mapper,
// is written without zod, and nothing removes it. Both run in the default build,
// only the guard runs in perf, and the difference between the modes is once
// again exactly "what is checked".
//
// A guard names the fields without which the mapper's output would be a crash or
// a fabrication:
//
//   crash        `toPlaceView` reads `positions.length` and `categories.map`
//                without asking, so a row lacking either is a TypeError.
//   fabrication  the same mapper derives x/y/left/top from `base_position`, and
//                `parseCoords` reads an absent one as 0,0 -- a card asserting a
//                parcel at the origin nobody deployed. schemas/places.ts states
//                that rule for the checking build; the guard is it, restated
//                where the stub cannot reach.
//
// Everything else is decoration and degrades on its own (`title || "Untitled
// parcel"`), so it stays out. A guard is not a second copy of the shape: of
// `PlaceSchema`'s nineteen fields, `isRenderablePlace` names four.
//
// Two properties keep a guard from drifting into one, and rows.test.ts asserts
// both:
//
//   A guard accepts every row its schema accepted. So it never fires in the
//   default build, the default build's output is bit-for-bit what it was, and
//   the perf-parity gate stays green by construction -- a guard is a perf-mode
//   backstop, not a new rejection. `isRenderablePlaceCategory` is the one
//   deliberate exception, and only because it absorbs a gate the reader was
//   already applying by hand.
//
//   Every mapper survives the MINIMAL row its guard admits. This is the one that
//   catches real drift: a mapper that grows a new unconditional read fails it,
//   because the minimal row is built from the guard's requirements and nothing
//   else. A field added to a schema needs no guard change and does not fail it,
//   which is the point -- the guard tracks the mapper, not the shape.

import type { ZodType } from "zod";

/** Answers "can the mapper use this row", never "is this row correct". */
export type RowGuard = (row: unknown) => boolean;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * One field of a value a stub may have waved through as anything at all,
 * `null` included -- which plain `.data` throws on.
 */
export function field(value: unknown, key: string): unknown {
  return isRecord(value) ? value[key] : undefined;
}

/**
 * The list an envelope was supposed to hold.
 *
 * In the default build the envelope schema already proved it is an array. In
 * perf nothing did, and `for (const row of undefined)` throws before the first
 * row is even looked at -- an envelope-shaped version of the same defect the
 * guards exist for. The cast is the caller's own declared element type: the
 * rows still face a schema and a guard before anything maps them.
 */
export function listOf<T>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}

/**
 * Identity only -- for a row whose mappers are already total and whose one
 * indispensable field is the id it is keyed, addressed and de-duplicated by.
 * A row missing a decoration renders thin; a row missing its id renders under a
 * duplicate key and can never be fetched again.
 */
export function hasId(row: unknown): boolean {
  return isRecord(row) && typeof row.id === "string";
}

/**
 * Schema then guard, in that order, on one row. `null` means "not usable".
 *
 * The guard reads `parsed.data` rather than `raw` because the default build
 * strips undeclared keys, and the subject of the question is whatever the mapper
 * will actually be handed.
 */
export function keepRow<W>(raw: unknown, schema: ZodType<W>, guard: RowGuard): W | null {
  const parsed = schema.safeParse(raw);
  if (!parsed.success) return null;
  return guard(parsed.data) ? parsed.data : null;
}

/** `keepRow` over a list, mapping what survives and dropping what does not. */
export function keepRows<W, T>(
  raw: unknown,
  schema: ZodType<W>,
  guard: RowGuard,
  map: (row: W) => T,
): T[] {
  const out: T[] = [];
  for (const item of listOf(raw)) {
    const row = keepRow(item, schema, guard);
    if (row !== null) out.push(map(row));
  }
  return out;
}
