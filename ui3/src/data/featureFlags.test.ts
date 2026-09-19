import { afterEach, expect, test, vi } from "vitest";
import { FEATURE_FLAGS, flagState, flagStorageKey, saveFlagOverride } from "./featureFlags";

afterEach(() => { vi.restoreAllMocks(); for (const flag of FEATURE_FLAGS) localStorage.removeItem(flagStorageKey(flag.id)); history.replaceState(null, "", "/"); });

test.each(FEATURE_FLAGS)("$id defaults on, with URL taking priority over the browser override", ({ id }) => {
  expect(flagState(id)).toEqual({ enabled: true, override: "default", source: "default" });
  localStorage.setItem(flagStorageKey(id), "0");
  expect(flagState(id).enabled).toBe(false);
  history.replaceState({ test: true }, "", `/?${id}=1&realm=scene&preview=true#/settings?section=flags`);
  expect(flagState(id).source).toBe("url");
  expect(flagState(id).enabled).toBe(true);
  saveFlagOverride(id, "disabled");
  expect(flagState(id)).toEqual({ enabled: false, override: "disabled", source: "browser" });
  expect(location.search).toBe("?realm=scene&preview=true");
  expect(location.hash).toBe("#/settings?section=flags");
  expect(history.state).toEqual({ test: true });
  saveFlagOverride(id, "default");
  expect(localStorage.getItem(flagStorageKey(id))).toBeNull();
  expect(flagState(id).enabled).toBe(true);
});

test("blocked storage retains defaults and reports write failure instead of claiming success", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw Error("blocked"); });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw Error("blocked"); });
  expect(flagState("2026-09-unity-shaders").enabled).toBe(true);
  expect(() => saveFlagOverride("2026-09-unity-shaders", "disabled")).toThrow("blocked");
});
