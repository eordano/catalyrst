export const LOADING_TRANSFER_EVENT = "dcl:loading-transfers";
export const LOADING_TRANSFER_MESSAGE = "ASSET_DOWNLOAD_PROGRESS";

export type LoadingTransfers = {
  started: number;
  completed: number;
  failed: number;
  receivedBytes: number;
  active: number;
  unknownActive: number;
  remainingBytes: number;
};

export const EMPTY_TRANSFERS: LoadingTransfers = {
  started: 0, completed: 0, failed: 0, receivedBytes: 0,
  active: 0, unknownActive: 0, remainingBytes: 0,
};

export function isLoadingTransfers(value: unknown): value is LoadingTransfers {
  return value != null && typeof value === "object" && Object.keys(EMPTY_TRANSFERS).every(key => {
    const n = (value as Record<string, unknown>)[key];
    return typeof n === "number" && Number.isFinite(n) && n >= 0;
  });
}

declare global {
  interface Window { __dclLoadingTransfers?: LoadingTransfers; }
}
