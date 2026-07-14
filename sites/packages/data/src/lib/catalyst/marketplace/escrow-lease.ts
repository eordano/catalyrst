import { UsageGrantStatusSchema } from "../generated-schemas/market";

/**
 * View shape shared by both sources `readLease` reads: a nested wire object
 * (validated by the generated `UsageGrantStatusSchema`, catalyrst-market's
 * ts-rs image) and flat item fields. `null` means "not reported by that
 * source", so the flat path can still describe a lease without a urn.
 */
export type EscrowLease = {
  urn: string | null;
  tokenId: string | null;
  category: string | null;
  status: string;
  unlockAt: number;
};

export type LeasableItem = {
  urn?: string | null;
  tokenId?: string | null;
  category?: string | null;
  status?: string | null;
  unlockAt?: number | null;
  lease?: unknown;
  usageGrant?: unknown;
};

export function readLease(item: LeasableItem | null | undefined): EscrowLease | null {
  if (!item) return null;

  const nested = item.lease ?? item.usageGrant;
  if (nested) {
    const r = UsageGrantStatusSchema.safeParse(nested);
    if (r.success) {
      return {
        urn: r.data.urn,
        tokenId: r.data.tokenId ?? null,
        category: r.data.category,
        status: r.data.status,
        unlockAt: r.data.unlockAt,
      };
    }
    // Salvage, mirroring the flat branch below: a trimmed nested object that
    // still asserts "leased" + a concrete unlock time must keep the return
    // window locked -- failing to recognise a lease would UN-gate Sell on an
    // escrowed item, which is the unsafe direction.
    if (typeof nested === "object") {
      const n = nested as Partial<Record<keyof EscrowLease, unknown>>;
      if (n.status === "leased" && typeof n.unlockAt === "number") {
        return {
          urn: typeof n.urn === "string" ? n.urn : null,
          tokenId: typeof n.tokenId === "string" ? n.tokenId : null,
          category: typeof n.category === "string" ? n.category : null,
          status: "leased",
          unlockAt: n.unlockAt,
        };
      }
    }
  }

  if (item.status === "leased" && typeof item.unlockAt === "number") {
    return {
      urn: item.urn ?? null,
      tokenId: item.tokenId ?? null,
      category: item.category ?? null,
      status: "leased",
      unlockAt: item.unlockAt,
    };
  }

  return null;
}

export function isInReturnWindow(
  lease: EscrowLease | null | undefined,
  now: number = Date.now(),
): boolean {
  if (!lease) return false;
  return lease.status === "leased" && lease.unlockAt > now;
}

export function availableAfterLabel(lease: EscrowLease): string {
  const d = new Date(lease.unlockAt);
  if (Number.isNaN(d.getTime())) return "soon";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function returnWindowMessage(lease: EscrowLease): string {
  return `In return window \u{2014} available to sell after ${availableAfterLabel(lease)}`;
}
