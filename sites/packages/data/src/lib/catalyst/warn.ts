export function warnInvalid(kind: string, issues: unknown): void {
  if (typeof process !== "undefined" && process.env?.NODE_ENV !== "production") {
    console.warn(`[catalyst] ${kind} failed schema validation`, issues);
  }
}
