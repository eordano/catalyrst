export const FOUNDATION_BUILDER = "https://builder-api.decentraland.org/v1";

type Session = {
  sign(method: string, path: string): Promise<{ headers: Record<string, string> }>;
  fetch(url: string, init?: RequestInit): Promise<Response>;
};

export function foundationFetch(session: () => Session) {
  return async (url: string, init: RequestInit = {}) => {
    if (!url.startsWith(`${FOUNDATION_BUILDER}/`)) return session().fetch(url, init);
    const path = new URL(url).pathname;
    const signed = await session().sign(init.method ?? "GET", path);
    const headers = new Headers(init.headers);
    for (const [key, value] of Object.entries(signed.headers)) headers.set(key, value);
    return fetch(`/api/builder-foundation${path.slice(3)}`, { ...init, headers });
  };
}
