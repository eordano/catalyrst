const PUBLIC_PORTAL_HOST = "https://portal.sqd.dev";
const SHARED_PORTAL_HOST = "https://shared.portal.sqd.dev";

export type PortalSource =
  | string
  | { url: string; http: { headers: Record<string, string> } };

/**
 * The SQD Network Portal stream this squid ingests from.
 *
 * Upstream (marketplace-squid-core #100/#109) moved both processors onto the SHARED, authenticated
 * Portal endpoint, and the reason is not cosmetic. A Portal query is capped at 256 KiB, and the
 * Polygon log filter carries the full list of DCL collection addresses -- ~260 KB today and growing
 * with every collection published. Over the PUBLIC endpoint that query is rejected with `400 Query
 * is too large` and the Polygon processor never ingests a single block, which is what stalled the
 * upstream Polygon reindex for two weeks. The shared endpoint raises the cap; it is authenticated,
 * so it also needs the key.
 *
 * LOCAL ADAPTATION -- diverges from upstream, which fails closed on a missing key. This deployment
 * wires no SQD_PORTAL_API_KEY (env/squid.env carries only the RPC endpoints), and fail-closing would
 * crash-loop BOTH processors on the next rebuild -- including the ETH L1 processor, whose small fixed
 * filter ingests from the public endpoint just fine today. So with no key we stay on the public
 * endpoint, byte-for-byte what we ingested before this change (the Polygon 256 KiB cap is the
 * pre-existing status quo, not a regression this introduces). Provision SQD_PORTAL_API_KEY and both
 * processors move onto the shared endpoint with an `x-api-key` header, no code change. The shared
 * host is overridable via SQD_PORTAL_URL because the endpoint has already moved once. Replace this
 * fallback with upstream's fail-closed assertNotNull once the key is a permanent part of the env.
 */
export function portalSource(dataset: string): PortalSource {
  const apiKey = process.env.SQD_PORTAL_API_KEY;
  if (!apiKey) {
    return `${PUBLIC_PORTAL_HOST}/datasets/${dataset}`;
  }
  const host = process.env.SQD_PORTAL_URL || SHARED_PORTAL_HOST;
  return {
    url: `${host}/datasets/${dataset}`,
    http: { headers: { "x-api-key": apiKey } },
  };
}
