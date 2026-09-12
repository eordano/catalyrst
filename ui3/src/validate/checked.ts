
import type { ZodType } from "zod";

export const VALIDATION_ENABLED = true;

const reported = new Set<string>();

const failures = new Map<string, number>();

export function validationFailures(): ReadonlyMap<string, number> {
  return failures;
}

export function resetValidationFailures(): void {
  failures.clear();
  reported.clear();
  reporter = null;
  devOverride = null;
}

export type ValidationReporter = (report: {
  boundary: string;
  detail: string;
  paths: string[];
}) => void;

let reporter: ValidationReporter | null = null;

export function setValidationReporter(next: ValidationReporter | null): void {
  reporter = next;
}

const INLINED_DEV = (() => {
  try {
    return Boolean((import.meta as { env?: { DEV?: boolean } }).env?.DEV);
  } catch {
    return false;
  }
})();

let devOverride: boolean | null = null;

export function setValidationDevMode(dev: boolean | null): void {
  devOverride = dev;
}

function isDev(): boolean {
  return devOverride ?? INLINED_DEV;
}

function redactPath(path: readonly PropertyKey[]): string {
  return path
    .map((seg) => {
      if (typeof seg !== "string") return String(seg);
      const identifying =
        /^0x[0-9a-fA-F]{6,}$/.test(seg) ||
        /^[0-9a-fA-F]{16,}$/.test(seg) ||
        /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-/.test(seg);
      return identifying ? "<key>" : seg;
    })
    .join(".");
}

function evaluate<T>(
  schema: ZodType<T>,
  value: unknown,
  boundary: string,
): { ok: boolean; data: T } {
  const result = schema.safeParse(value);
  if (result.success) return { ok: true, data: result.data };

  failures.set(boundary, (failures.get(boundary) ?? 0) + 1);

  const paths = result.error.issues.map((i) => redactPath(i.path));
  const detail = result.error.issues
    .slice(0, 3)
    .map((i, n) => `${paths[n] || "(root)"}: ${i.message}`)
    .join("; ");

  const first = !reported.has(boundary);
  if (first) reported.add(boundary);

  if (first && reporter) {
    try {
      reporter({ boundary, detail, paths });
    } catch {
    }
  }

  if (isDev()) {
    throw new Error(
      `validation failed at ${boundary} \u{2014} ${detail}\n` +
        "If this is persisted state after a branch switch, the stored value was written by " +
        "an older build: clear it from localStorage and reload.",
    );
  }
  if (first) {
    console.warn(`[validate] ${boundary} \u{2014} ${detail} (further reports suppressed)`);
  }
  return { ok: false, data: value as T };
}

export function check<T>(schema: ZodType<T>, value: unknown, boundary: string): T {
  return evaluate(schema, value, boundary).data;
}

export function checkOk(schema: ZodType<unknown>, value: unknown, boundary: string): boolean {
  return evaluate(schema, value, boundary).ok;
}
