import type { QueryClient } from "@tanstack/react-query";
import type { ReelPhoto } from "../../explorer/components/PhotoDetail";
import { serviceBase, signedFetch } from "./client";

export type ReelPage = { images: ReelPhoto[]; currentImages: number; maxImages: number };
export const reelKey = (address: string) => ["camera-reel", address.toLowerCase()];

export function reelQuery(address: string) {
  return {
    queryKey: reelKey(address),
    staleTime: 30_000,
    queryFn: async (): Promise<ReelPage> => {
      const { status, body } = await signedFetch(`${serviceBase("cameraReel")}/api/users/${encodeURIComponent(address)}/images?limit=100&offset=0`, { method: "GET" });
      if (status < 200 || status >= 300) throw new Error("Gallery unavailable");
      const data = JSON.parse(body);
      if (!Array.isArray(data?.images)) throw new Error("Invalid gallery response");
      return {
        images: data.images,
        currentImages: Number.isFinite(data.currentImages) ? data.currentImages : data.images.length,
        maxImages: Number.isFinite(data.maxImages) ? data.maxImages : 0,
      };
    },
  };
}

export function prefetchReel(client: QueryClient, address?: string | null) {
  return address ? client.prefetchQuery(reelQuery(address)) : Promise.resolve();
}
