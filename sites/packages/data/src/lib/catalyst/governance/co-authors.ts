import { isEthAddress } from "../format/address";

export type FieldErrors = Record<string, string>;

/**
 * Shared by every governance submit-*.ts co-author field: length-capped at
 * `max`, each non-blank entry must be a wallet address. Blank entries are
 * skipped (not flagged) -- callers that need to reject blanks outright should
 * filter before calling this.
 */
export function validateCoAuthors(coAuthors: string[], max: number): FieldErrors {
  const errors: FieldErrors = {};
  if (coAuthors.length > max) {
    errors.coAuthors = `You can add at most ${max} co-authors.`;
    return errors;
  }
  for (const addr of coAuthors) {
    const trimmed = addr.trim();
    if (trimmed !== "" && !isEthAddress(trimmed)) {
      errors.coAuthors = "Co-author must be a wallet address.";
      break;
    }
  }
  return errors;
}
