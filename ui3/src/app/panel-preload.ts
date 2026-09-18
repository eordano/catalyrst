import type { QueryClient } from "@tanstack/react-query";
import type { ComponentType } from "react";

export type PanelModule = {
  default: ComponentType;
  prefetch?: (client: QueryClient, address?: string | null) => unknown;
};

export function createPanelPreloader(loaders: Record<string, () => Promise<PanelModule>>) {
  const modules = new Map<string, Promise<PanelModule>>();
  const preload = async (client: QueryClient, id: string, address?: string | null) => {
    id = id.split("?")[0]!;
    const loader = loaders[id];
    if (!loader) return;
    let pending = modules.get(id);
    if (!pending) {
      pending = loader().catch(error => { modules.delete(id); throw error; });
      modules.set(id, pending);
    }
    try {
      const module = await pending;
      await module.prefetch?.(client, address);
    } catch {
      // Intent is speculative; opening the panel owns visible error recovery.
    }
  };
  preload.all = (client: QueryClient, address?: string | null) =>
    Promise.all(Object.keys(loaders).map(id => preload(client, id, address)));
  return preload;
}
