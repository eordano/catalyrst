import { isEthAddress } from "../format/address";

export type FieldErrors = Record<string, string>;

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
