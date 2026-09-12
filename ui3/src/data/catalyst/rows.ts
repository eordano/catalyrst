
import type { ZodType } from "zod";

export type RowGuard = (row: unknown) => boolean;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function field(value: unknown, key: string): unknown {
  return isRecord(value) ? value[key] : undefined;
}

export function listOf<T>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}

export function hasId(row: unknown): boolean {
  return isRecord(row) && typeof row.id === "string";
}

export function keepRow<W>(raw: unknown, schema: ZodType<W>, guard: RowGuard): W | null {
  const parsed = schema.safeParse(raw);
  if (!parsed.success) return null;
  return guard(parsed.data) ? parsed.data : null;
}

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
