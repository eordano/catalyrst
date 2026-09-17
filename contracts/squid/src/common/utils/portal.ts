const PUBLIC_PORTAL_HOST = "https://portal.sqd.dev";
const SHARED_PORTAL_HOST = "https://shared.portal.sqd.dev";

export type PortalSource = {
  url: string;
  http: { retryAttempts: number; headers?: Record<string, string> };
};

export function publicPortalDataset(dataset: string): string {
  return `${PUBLIC_PORTAL_HOST}/datasets/${dataset}`;
}

export function portalSource(dataset: string): PortalSource {
  const apiKey = process.env.SQD_PORTAL_API_KEY;
  if (!apiKey) {
    const consequence = dataset.startsWith("polygon-")
      ? "The Polygon collection-address filter is at the 256 KiB query cap, " +
        "so this processor will be rejected with 400 Query is too large and " +
        "retry forever."
      : `The ${dataset} processor runs unauthenticated: expect public-tier ` +
        "rate limits and no quota guarantees.";
    console.error(
      "[PORTAL] SQD_PORTAL_API_KEY is not set: falling back to the " +
        "unauthenticated public Portal. " +
        consequence
    );
    return {
      url: publicPortalDataset(dataset),
      http: { retryAttempts: Infinity },
    };
  }
  const host = process.env.SQD_PORTAL_URL || SHARED_PORTAL_HOST;
  return {
    url: `${host}/datasets/${dataset}`,
    http: { retryAttempts: Infinity, headers: { "x-api-key": apiKey } },
  };
}
