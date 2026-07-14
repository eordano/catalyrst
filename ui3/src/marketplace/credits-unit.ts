export function isOneCredit(value: unknown): boolean {
  if (typeof value === "number") return value === 1;
  if (typeof value === "string") {
    const n = Number(value.trim().replace(/,/g, ""));
    return n === 1;
  }
  return false;
}

export function creditsNoun(value: unknown, capitalized = false): string {
  const one = isOneCredit(value);
  if (capitalized) return one ? "Credit" : "Credits";
  return one ? "credit" : "credits";
}

export function creditsSrLabel(value: unknown): string {
  return " " + creditsNoun(value);
}
