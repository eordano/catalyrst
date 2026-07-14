export type Relationship = "none" | "requested" | "incoming" | "friend" | "blocked";

type AddressRef = { address: string };

export function relationshipOf(
  friends: readonly AddressRef[],
  received: readonly AddressRef[],
  sent: readonly AddressRef[],
  blocked: readonly AddressRef[],
  address: string,
): Relationship {
  const a = address.toLowerCase();
  const has = (list: readonly AddressRef[]) => list.some((r) => r.address.toLowerCase() === a);
  if (has(blocked)) return "blocked";
  if (has(friends)) return "friend";
  if (has(received)) return "incoming";
  if (has(sent)) return "requested";
  return "none";
}
