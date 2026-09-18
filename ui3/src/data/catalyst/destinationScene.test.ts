import { expect, test, vi } from "vitest";
import { destinationLabel, fetchDestinationScene } from "./destinationScene";

const place = {
  id: "target", title: "Cyberpunk Disparity", base_position: "10,-20", positions: ["10,-20", "11,-20"],
  image: "https://images.example/scene.png", categories: [], user_visits: 0, favorites: 0, likes: 0,
  highlighted: false, world: false,
};

test("resolves the scene containing the exact linked parcel, including a non-base parcel", async () => {
  const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ ok: true, data: [place], total: 1 })));
  const scene = await fetchDestinationScene({ coords: "11,-20" }, { fetchImpl });
  expect(scene?.title).toBe("Cyberpunk Disparity");
  const url = new URL(String(fetchImpl.mock.calls[0]?.[0]));
  expect(url.pathname).toBe("/places/api/places");
  expect(url.searchParams.get("positions")).toBe("11,-20");
});

test("resolves a named world link by name rather than its default Genesis coordinates", async () => {
  const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ ok: true, data: [{ ...place, world: true, world_name: "kickoff.dcl.eth" }], total: 1 })));
  await fetchDestinationScene({ realm: "https://worlds.example/world/kickoff.dcl.eth/about", coords: "4,5" }, { fetchImpl });
  const url = new URL(String(fetchImpl.mock.calls[0]?.[0]));
  expect(url.pathname).toBe("/places/api/worlds");
  expect(url.searchParams.get("names")).toBe("kickoff.dcl.eth");
  expect(url.searchParams.has("positions")).toBe(false);
});

test("unlisted parcels use deployed scene metadata and resolve thumbnail content", async () => {
  const fetchImpl = vi.fn<typeof fetch>()
    .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, data: [], total: 0 })))
    .mockResolvedValueOnce(new Response(JSON.stringify([{ pointers: ["1,2"], metadata: { display: { title: "Private workshop", navmapThumbnail: "thumb.png" } }, content: [{ file: "thumb.png", hash: "bafyimage" }] }])));
  await expect(fetchDestinationScene({ coords: "1,2" }, { fetchImpl, base: "https://realm.example" }))
    .resolves.toEqual({ title: "Private workshop", image: "https://realm.example/content/contents/bafyimage", creator: null });
});

test("unlisted worlds resolve their scene URN without starting the engine", async () => {
  const fetchImpl = vi.fn<typeof fetch>()
    .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, data: [], total: 0 })))
    .mockResolvedValueOnce(new Response(JSON.stringify({ configurations: { scenesUrn: ["urn:decentraland:entity:bafyscene?baseUrl=https://content.example/contents/"] } })))
    .mockResolvedValueOnce(new Response(JSON.stringify({ metadata: { display: { title: "World workshop" } } })));
  await expect(fetchDestinationScene({ realm: "workshop.dcl.eth" }, { fetchImpl })).resolves.toMatchObject({ title: "World workshop" });
  expect(String(fetchImpl.mock.calls[2]?.[0])).toBe("https://content.example/contents/bafyscene");
});

test("missing metadata still names the requested destination", () => {
  expect(destinationLabel({ coords: "-29,55" })).toBe("Parcel -29,55");
  expect(destinationLabel({ realm: "kickoff.dcl.eth" })).toBe("kickoff.dcl.eth");
});
