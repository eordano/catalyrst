const PUBLIC_PORTAL_HOST = "https://portal.sqd.dev";
const SHARED_PORTAL_HOST = "https://shared.portal.sqd.dev";

export type PortalSource =
  | string
  | {
      url: string;
      http: { retryAttempts: number; headers?: Record<string, string> };
    };

export function publicPortalDataset(dataset: string): string {
  return `${PUBLIC_PORTAL_HOST}/datasets/${dataset}`;
}

export function portalSource(dataset: string): PortalSource {
  const apiKey = process.env.SQD_PORTAL_API_KEY;
  if (!apiKey) {
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
