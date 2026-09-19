import { afterEach, expect, test, vi } from "vitest";
import { sidebarDesignEnabled, SIDEBAR_DESIGN_FLAG, SIDEBAR_DESIGN_STORAGE } from "./sidebarDesignFlag";

afterEach(() => { localStorage.removeItem(SIDEBAR_DESIGN_STORAGE); history.replaceState(null, "", "/"); vi.restoreAllMocks(); });

test("the redesign is on by default and accepts an explicit URL opt-in", () => {
  expect(sidebarDesignEnabled()).toBe(true);
  history.replaceState(null, "", `/?${SIDEBAR_DESIGN_FLAG}=1`);
  expect(sidebarDesignEnabled()).toBe(true);
  expect(localStorage.getItem(SIDEBAR_DESIGN_STORAGE)).toBeNull();
});

test("a URL override wins over a saved preference, including an explicit opt-out", () => {
  localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1");
  expect(sidebarDesignEnabled()).toBe(true);
  history.replaceState(null, "", `/?${SIDEBAR_DESIGN_FLAG}=0`);
  expect(sidebarDesignEnabled()).toBe(false);
  history.replaceState(null, "", `/?${SIDEBAR_DESIGN_FLAG}=unexpected`);
  expect(sidebarDesignEnabled()).toBe(false);
});

test("blocked browser storage keeps the release default", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
  expect(sidebarDesignEnabled()).toBe(true);
});
