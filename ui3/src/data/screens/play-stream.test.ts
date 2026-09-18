import { expect, it, vi } from "vitest";
import { readPlayStream } from "./play-stream";

const names = ["featured", "places", "events", "wearables", "emotes", "outfits"];
const unavailable = (section: string) => ({
  version: 1, address: "", section,
  result: { status: "unavailable", data: null, updatedAt: null },
});
const done = { version: 1, address: "", done: true };

function stream(frames: unknown[], fragment = false) {
  const bytes = new TextEncoder().encode(frames.map((frame) => JSON.stringify(frame)).join("\n"));
  return new Response(new ReadableStream({ start(controller) {
    for (let i = 0; i < bytes.length; i += fragment ? 7 : bytes.length) {
      controller.enqueue(bytes.slice(i, i + (fragment ? 7 : bytes.length)));
    }
    controller.close();
  } }), { headers: { "content-type": "application/x-ndjson" } });
}

it("decodes fragmented frames including the final frame without a newline", async () => {
  const receive = vi.fn();
  const data = await readPlayStream(stream([...names.map(unavailable), done], true), "", receive);
  expect(Object.keys(data.sections)).toEqual(names);
  expect(receive).toHaveBeenCalledTimes(6);
});

it("accepts an opt-in upcoming section without requiring it from older servers", async () => {
  const receive = vi.fn();
  const data = await readPlayStream(stream([...names.map(unavailable), unavailable("upcoming"), done], true), "", receive);
  expect(data.sections.upcoming?.status).toBe("unavailable");
  expect(receive).toHaveBeenCalledTimes(7);
});

it.each([
  [unavailable("featured")],
  [unavailable("featured"), done],
  [unavailable("featured"), unavailable("featured")],
  [...names.map(unavailable), done, unavailable("places")],
  [{ ...unavailable("featured"), address: "another-wallet" }],
  [{ ...unavailable("featured"), result: { status: "ready", data: {}, updatedAt: 1 } }],
])("rejects truncated, duplicate, misplaced, or malformed frames %#", async (...frames) => {
  await expect(readPlayStream(stream(frames), "", () => {})).rejects.toThrow();
});

it.each([false, true])("normalizes public images for aggregated responses (stream=%s)", async streamed => {
  const featured = { ...unavailable("featured"), result: { status: "ready", updatedAt: 1, data: [{ id: "place", image: "https://marketing-files.decentraland.org/uploads/banner.png" }] } };
  const frames = names.map(name => name === "featured" ? featured : unavailable(name));
  const response = streamed ? stream([...frames, done]) : Response.json({ version: 1, address: "", sections: Object.fromEntries(frames.map(frame => [frame.section, frame.result])) });
  const data = await readPlayStream(response, "", () => {});
  expect(data.sections.featured.data?.[0]?.image).toBe(`${window.location.origin}/marketing-files/uploads/banner.png`);
});
