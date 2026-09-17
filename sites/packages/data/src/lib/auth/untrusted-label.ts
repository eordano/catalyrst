const UNREADABLE_CHARACTER_RE = /(?![\t\n\r])[\\\p{C}\p{Zl}\p{Zp}\uFFFD]/gu;

export function revealUnreadableCharacters(text: string): string {
  return text.replace(UNREADABLE_CHARACTER_RE, (character) =>
    character === "\\" ? "\\\\" : `\\u{${(character.codePointAt(0) ?? 0).toString(16)}}`,
  );
}

export const MAX_UNTRUSTED_LABEL_CHARS = 40;

export function formatUntrustedLabel(
  value: unknown,
  maxLength: number = MAX_UNTRUSTED_LABEL_CHARS,
): string {
  if (typeof value !== "string") return "";
  const revealed = revealUnreadableCharacters(value.trim());
  if (revealed.length <= maxLength) return revealed;
  let shown = "";
  for (const character of revealed) {
    if (shown.length + character.length > maxLength - 1) break;
    shown += character;
  }
  return `${shown}\u{2026}`;
}
