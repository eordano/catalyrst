// Pins the two catalog shapes placement must serve: the live builder catalog
// (glbUrl) and the bundled seed catalog (src). The seed path shipped placing
// EMPTY entities -- a named Transform with no GltfContainer -- because only
// glbUrl was read.
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

describe("placeAssetOnBus", () => {
  it("places a seed-catalog asset (src) with its model attached", async () => {
    const bus = fakeBus();
    await placeAssetOnBus(bus.ref, {
      id: "door", name: "Cyberpunk Door", pack: "Smart Items",
      src: "/content/contents/bafyDOOR", smart: true,
    });
    expect(bus.calls).toHaveLength(1);
    const c = bus.calls[0]!;
    expect(c.name).toBe("Cyberpunk Door");
    expect((c.components as { GltfContainer: { src: string } }).GltfContainer.src)
      .toContain("/content/contents/bafyDOOR");
  });

  it("still prefers the live catalog's glbUrl when both exist", async () => {
    const bus = fakeBus();
    await placeAssetOnBus(bus.ref, {
      id: "x", name: "X", glbUrl: "/builder-items/bafyLIVE", src: "/content/contents/bafySEED",
    });
    expect((bus.calls[0]!.components as { GltfContainer: { src: string } }).GltfContainer.src)
      .toContain("bafyLIVE");
  });
});
