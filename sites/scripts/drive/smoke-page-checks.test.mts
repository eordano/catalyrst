import { expect, it } from "vitest";
import { PAGE_CHECKS } from "./smoke-page-checks.mts";

it("distinguishes offscreen lazy images from failed and visible pending images", () => {
  const make = (src: string, top: number, complete = false, naturalWidth = 0, loading = "lazy") => ({
    src, complete, naturalWidth, loading,
    getBoundingClientRect: () => ({ top, bottom: top + 100, left: 0, right: 100 }),
  });
  const images = [
    make("deferred", 2200), make("loaded", 20, true, 640),
    make("visible-pending", 20), make("failed-offscreen", 2200, true),
    make("eager-pending", 2200, false, 0, "eager"),
  ];
  const check = new Function("document", "innerHeight", "innerWidth", `return ${PAGE_CHECKS};`);
  const result = JSON.parse(check({ images, fonts: { check: () => true }, title: "Places", body: { innerText: "Places" } }, 913, 1280));
  expect(result.broken).toEqual(["visible-pending", "failed-offscreen", "eager-pending"]);
});
