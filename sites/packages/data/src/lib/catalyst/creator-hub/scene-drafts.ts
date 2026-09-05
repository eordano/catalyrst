export type DraftMeta = {
  id: string;
  hash: string;
  version: number;
  updatedAt: number;
  title: string;
};

export type Draft = DraftMeta & { blob: unknown };

export type PutResult =
  | { ok: true; meta: DraftMeta }
  | { ok: false; conflict: true; server: Draft };

export type PutDraftInput = {
  baseVersion: number;
  hash: string;
  blob: unknown;
  title?: string;
};

export const WALLET_RE = /^0x[0-9a-f]{40}$/;

export const DRAFT_ID_RE = /^[a-z0-9_.,-]{1,200}$/i;

export function normalizeWallet(wallet: string): string {
  return typeof wallet === "string" ? wallet.trim().toLowerCase() : "";
}

export function isSafeWallet(wallet: string): boolean {
  return WALLET_RE.test(normalizeWallet(wallet));
}

export function isSafeDraftId(id: string): boolean {
  return (
    typeof id === "string" &&
    DRAFT_ID_RE.test(id) &&
    !id.includes("..") &&
    id !== "."
  );
}
