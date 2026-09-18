const REDIRECT_TARGET = "/create/wearables";

type RedirectResult = {
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
