import { describe, expect, it, vi } from "vitest";
import { loader as collection } from "./builder.collection_.$id";
import { loader as collections } from "./builder.collections_.$id";

vi.mock("@core/lib/telemetry/track", () => ({ track: vi.fn() }));

const owner = "0x366a0a523f640fe73830c302a68585b494e88cb5";
const id = "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925";

describe.each([["collection", collection], ["collections", collections]] as const)("builder/%s collection redirect", (segment, loader) => {
  it("preserves the collection owner and view when redirecting legacy links", async () => {
    const response = await loader({
      request: new Request(`https://sites.test/builder/${segment}/${id}?address=${owner}&tab=items&variant=detail&unrelated=discard`),
      params: { id },
      context: {} as never,
    } as never);
    expect(response.status).toBe(308);
    expect(response.headers.get("Location")).toBe(`/create/wearables/collections/${id}?address=${owner}&tab=items&variant=detail`);
  });
});
