// The performance-mode implementation: `check` is the identity.
//
// It takes `unknown` for the schema rather than `ZodType<T>` on purpose. The
// perf build also aliases the schema modules to stubs whose exports are a
// two-method shim that accepts everything, so the argument arriving here is not
// a schema at all -- and typing it as one would make this file the only thing in
// the tree still importing zod's types, which is exactly what the mode exists
// to remove.
//
// The signature is otherwise identical to checked.ts, and validate/index.ts
// re-exports whichever one the build selected under the same name, so no call
// site can tell which is present.

export const VALIDATION_ENABLED = false;

export function validationFailures(): ReadonlyMap<string, number> {
  return new Map();
}

export function resetValidationFailures(): void {}

export type ValidationReporter = (report: {
  boundary: string;
  detail: string;
  paths: string[];
}) => void;

// Accepted and dropped: nothing can fail, so nothing can be reported. Present
// only so installing a reporter is not a build error in perf mode.
export function setValidationReporter(_next: ValidationReporter | null): void {}

// Same reason as above: nothing validates, so there is no branch to force.
export function setValidationDevMode(_dev: boolean | null): void {}

export function check<T>(_schema: unknown, value: unknown, _boundary: string): T {
  return value as T;
}

// Nothing is validated, so nothing can fail: a caller that skips on false never
// skips here. That is the trade perf mode makes, stated once rather than per
// call site.
export function checkOk(_schema: unknown, _value: unknown, _boundary: string): boolean {
  return true;
}
