import { expect, it } from "vitest";
import { PAGE_CHECKS } from "./smoke-page-checks.mts";

const make = (src: string, top: number, complete = false, naturalWidth = 0, loading = "lazy") => ({
  src, complete, naturalWidth, loading,
  getBoundingClientRect: () => ({ top, bottom: top + 100, left: 0, right: 100 }),
});

const run = (images: ReturnType<typeof make>[]) => {
  const check = new Function("document", "innerHeight", "innerWidth", `return ${PAGE_CHECKS};`);
  return JSON.parse(check({ images, fonts: { check: () => true }, title: "Places", body: { innerText: "Places" } }, 913, 1280));
};

it("distinguishes offscreen lazy images from failed and visible pending images", () => {
  const result = run([
    make("deferred", 2200), make("loaded", 20, true, 640),
    make("visible-pending", 20), make("failed-offscreen", 2200, true),
    make("eager-pending", 2200, false, 0, "eager"),
  ]);
  expect(result.broken).toEqual(["visible-pending", "failed-offscreen", "eager-pending"]);
  expect(result.brokenTotal).toBe(3);
});

it("names every broken image on a page with more than ten of them", () => {
  const failed = Array.from({ length: 40 }, (_, n) => make(`thumb-${n}`, 20 + n, true));
  const result = run([make("loaded", 20, true, 640), ...failed]);
  expect(result.brokenTotal).toBe(40);
  expect(result.broken).toEqual(failed.map((i) => i.src));
});
