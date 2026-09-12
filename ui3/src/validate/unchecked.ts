
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

export function setValidationReporter(_next: ValidationReporter | null): void {}

export function setValidationDevMode(_dev: boolean | null): void {}

export function check<T>(_schema: unknown, value: unknown, _boundary: string): T {
  return value as T;
}

export function checkOk(_schema: unknown, _value: unknown, _boundary: string): boolean {
  return true;
}
