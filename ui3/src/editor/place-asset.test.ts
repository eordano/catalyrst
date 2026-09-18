import { describe, expect, it } from "vitest";
import { placeAssetOnBus } from "./project-cache";

function fakeBus() {
  const calls: Array<{ name: string; parent: number; components: unknown }> = [];
  return {
    calls,
    ref: {
      current: {
        addEntity: (name: string, parent: number, components: unknown) =>
          calls.push({ name, parent, components }),
      },
    } as never,
  };
}

const src = (c: unknown) => (c as { GltfContainer: { src: string } }).GltfContainer.src;

describe("placeAssetOnBus", () => {
  it("places a seed-catalog asset with its model attached, preferring the live catalog's glbUrl when both exist", async () => {
    const bus = fakeBus();
    await placeAssetOnBus(bus.ref, {
      id: "door", name: "Cyberpunk Door", pack: "Smart Items",
      src: "/content/contents/bafyDOOR", smart: true,
    });
    await placeAssetOnBus(bus.ref, {
      id: "x", name: "X", glbUrl: "/builder-items/bafyLIVE", src: "/content/contents/bafySEED",
    });
    expect(bus.calls).toHaveLength(2);
    expect(bus.calls[0]!.name).toBe("Cyberpunk Door");
    expect(src(bus.calls[0]!.components)).toContain("/content/contents/bafyDOOR");
    expect(src(bus.calls[1]!.components)).toContain("bafyLIVE");
  });
});
