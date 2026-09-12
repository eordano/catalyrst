
function formEncode(value: string): string {
  return encodeURIComponent(value).replace(/[!'()~]/g, (c) =>
    `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

export function realmDeepLink(realm: string, position: string): string {
  return `decentraland://realm=${formEncode(realm)}&position=${formEncode(position)}`;
}

export function worldRealmUrl(worldsBaseUrl: string, worldName: string): string {
  return `${worldsBaseUrl.replace(/\/+$/, "")}/world/${worldName.toLowerCase()}`;
}
