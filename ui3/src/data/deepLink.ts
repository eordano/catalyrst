/**
 * Launch links for the desktop explorer.
 *
 * The TypeScript twin of `catalyrst-types::deep_link`; the encoding rules must
 * match, because the two emit the same link from opposite sides of the stack.
 *
 * `https://decentraland.org/play/?realm=...` cannot reach a self-hosted node:
 * the website forwards `realm` only for realms it trusts and drops it silently
 * otherwise, so the visitor boots the default Genesis realm at the requested
 * coordinates. Handing the protocol handler the link directly bypasses that.
 */

function formEncode(value: string): string {
  return encodeURIComponent(value).replace(/[!'()~]/g, (c) =>
    `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

export function realmDeepLink(realm: string, position: string): string {
  return `decentraland://realm=${formEncode(realm)}&position=${formEncode(position)}`;
}

/** Worlds are addressed by path off the worlds server, never by name alone. */
export function worldRealmUrl(worldsBaseUrl: string, worldName: string): string {
  return `${worldsBaseUrl.replace(/\/+$/, "")}/world/${worldName.toLowerCase()}`;
}
