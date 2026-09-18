import { z } from "zod";
import { draftRequest, type DraftOptions } from "./drafts";
import { FOUNDATION_BUILDER } from "./foundation-fetch";

export const linkedProviderId = z.string().max(256).regex(/^urn:decentraland:matic:collections-thirdparty:[^:|\s]+$/);
const Provider = z.object({ id: linkedProviderId, name: z.string(), managers: z.array(z.string()), published: z.boolean() });
export type LinkedProvider = z.infer<typeof Provider>;

export async function listLinkedProviders(address: string, opts: DraftOptions): Promise<LinkedProvider[]> {
  const providers = z.array(Provider).parse(await draftRequest("/thirdParties", { ...opts, base: FOUNDATION_BUILDER }));
  return providers.filter(provider => provider.managers.some(manager => manager.toLowerCase() === address.toLowerCase()));
}

export async function linkedProviderSlots(id: string, opts: DraftOptions): Promise<number> {
  linkedProviderId.parse(id);
  return z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER).parse(
    await draftRequest(`/thirdParties/${encodeURIComponent(id)}/slots`, { ...opts, base: FOUNDATION_BUILDER }),
  );
}
