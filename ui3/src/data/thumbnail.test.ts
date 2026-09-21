import { afterEach, expect, test, vi } from "vitest";
import { publicThumbnail } from "./thumbnail";

afterEach(() => vi.unstubAllEnvs());

test("SSR thumbnails use the deployment origin", () => {
  vi.stubEnv("SSR", true);
  vi.stubEnv("CATALYST_URL", "https://interconnected.online/");
  expect(publicThumbnail("https://worlds-content-server.decentraland.org/contents/bafytest"))
    .toBe("https://interconnected.online/media/convert?width=640&url=https%3A%2F%2Fworlds-content-server.decentraland.org%2Fcontents%2Fbafytest");
});

test("known public image sources use bounded thumbnails without rewriting private, signed or animated images", () => {
  const source = "https://peer-ec1.decentraland.org/content/contents/bafytest";
  const result = new URL(publicThumbnail(source, 320)!);
  expect(result.pathname).toBe("/media/convert");
  expect(result.searchParams.get("width")).toBe("320");
  expect(result.searchParams.get("url")).toBe(source);
  expect(publicThumbnail("https://events-assets-099ac00.decentraland.org/poster/poster.webp")).toContain("/media/convert?width=640");
  for (const url of ["/private/image", "data:image/png;base64,AA==", "https://example.com/photo.jpg", `${source}?signature=secret`, "https://events-assets-099ac00.decentraland.org/poster/poster.gif"]) expect(publicThumbnail(url)).toBe(url);
});
