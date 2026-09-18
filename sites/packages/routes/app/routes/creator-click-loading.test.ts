import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadCreatorScenes } from "@data/lib/catalyst/create/index.server";
import { loadCreatorMetrics } from "@data/lib/catalyst/creator-hub/metrics.server";
import { loadCreatorItem } from "@data/lib/catalyst/creator-hub/wearable-item-detail.server";
import { fetchCollections } from "@data/lib/catalyst/builder/collections";
import { probeSources } from "@data/lib/catalyst/creator-hub/data-sources.server";

const gate = vi.hoisted(() => ({ wait: Promise.resolve(), release: () => {} }));
vi.mock("@core/lib/experiments/assign", async importOriginal => ({
  ...await importOriginal<typeof import("@core/lib/experiments/assign")>(),
  resolveAssignment: vi.fn(async () => {
    await gate.wait;
    return { variant: "test", flags: {}, experimentKey: "click-loading" };
  }),
}));
vi.mock("@data/lib/catalyst/create/index.server", () => ({ loadCreatorScenes: vi.fn(async () => []) }));
vi.mock("@data/lib/catalyst/creator-hub/metrics.server", () => ({ loadCreatorMetrics: vi.fn(async () => null) }));
vi.mock("@data/lib/catalyst/creator-hub/wearable-item-detail.server", () => ({ loadCreatorItem: vi.fn(async () => ({ item: null, fallback: false })) }));
vi.mock("@data/lib/catalyst/builder/collections", async importOriginal => ({
  ...await importOriginal<typeof import("@data/lib/catalyst/builder/collections")>(),
  fetchCollections: vi.fn(async () => []),
}));
vi.mock("@data/lib/catalyst/creator-hub/data-sources.server", async importOriginal => ({
  ...await importOriginal<typeof import("@data/lib/catalyst/creator-hub/data-sources.server")>(),
  probeSources: vi.fn(async () => []),
}));

beforeEach(() => { gate.wait = new Promise<void>(resolve => { gate.release = resolve; }); });
afterEach(() => { gate.release(); vi.clearAllMocks(); });

const address = "0x1111111111111111111111111111111111111111";
const cases = [
  ["scenes", () => import("./create.scenes"), loadCreatorScenes, `?creator=${address}`],
  ["metrics", () => import("./creator-hub.metrics"), loadCreatorMetrics, `?address=${address}`],
  ["item detail", () => import("./create.wearables.items_.$id"), loadCreatorItem, ""],
  ["item editor", () => import("./create.wearables.item-editor"), fetchCollections, `?address=${address}`],
  ["data sources", () => import("./creator-hub.data-sources"), probeSources, ""],
] as const;

it.each(cases)("starts %s content before experiment flags finish", async (_name, module, read, search) => {
  const { loader } = await module();
  const request = new Request(`https://sites.test/create${search}`);
  const result = loader({ request, params: { id: "item" }, context: {} } as never);
  try {
    await Promise.resolve();
    expect(read).toHaveBeenCalledTimes(1);
  } finally {
    gate.release();
    await result;
  }
});
