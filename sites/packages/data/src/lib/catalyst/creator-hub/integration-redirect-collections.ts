export const REDIRECT_TARGET = "/create/wearables";

export type RedirectRow = {
  from: string;
  to: string;
  reason: string;
  note: string;
};

export type RedirectMap = {
  target: string;
  redirects: RedirectRow[];
  status: number;
};

export type RedirectResult = {
  location: string;
  to: string;
  query: string;
};

export function buildRedirect(requestUrl: string): RedirectResult {
  const search = new URL(requestUrl).search;
  const query = search.startsWith("?") ? search.slice(1) : search;
  return {
    location: `${REDIRECT_TARGET}${search}`,
    to: REDIRECT_TARGET,
    query,
  };
}
