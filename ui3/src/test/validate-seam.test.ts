import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { z } from "zod";

import {
  check,
  resetValidationFailures,
  setValidationDevMode,
  setValidationReporter,
  validationFailures,
} from "../validate";

// The seam's failure policy is used by all 16 check() call sites, so one set of
// tests protects every boundary. None of this was covered before: the reporter
// call, the once-per-boundary suppression and the return-the-original rule
// could all be deleted with both suites green.
//
// Vite inlines import.meta.env.DEV as a literal, so neither vi.stubEnv nor a
// direct assignment can reach the branch inside checked.ts -- both were tried
// and both silently kept the dev path. setValidationDevMode is the hook that
// makes the production branch reachable at all.

const Schema = z.object({ a: z.string() });

// Assigned directly rather than through vi.stubEnv, which coerces to a string:
// stubEnv("DEV", false) yields "false", and Boolean("false") is true, so the
// production branch never ran and the two tests below failed against the dev
// throw. Worth knowing before reaching for stubEnv on any boolean env flag.


beforeEach(() => {
  resetValidationFailures();
  setValidationDevMode(true);
});
afterEach(() => {
  resetValidationFailures();
  vi.restoreAllMocks();
});

describe("check", () => {
  test("returns parsed data and counts nothing on success", () => {
    expect(check(Schema, { a: "x" }, "t/ok")).toEqual({ a: "x" });
    expect(validationFailures().size).toBe(0);
  });

  test("throws in dev, and the message names the persisted-state cause", () => {
    expect(() => check(Schema, { a: 1 }, "t/bad")).toThrow(/validation failed at t\/bad/);
    // The branch-switch hint is the whole reason persisted state can keep the
    // dev throw without being mystifying.
    expect(() => check(Schema, { a: 1 }, "t/bad")).toThrow(/older build/);
  });

  test("counts a failure even though it throws", () => {
    expect(() => check(Schema, { a: 1 }, "t/counted")).toThrow();
    expect(validationFailures().get("t/counted")).toBe(1);
  });

  test("reports paths and never the value", () => {
    const seen: { boundary: string; detail: string; paths: string[] }[] = [];
    setValidationReporter((r) => seen.push(r));
    expect(() => check(Schema, { a: "secret-wallet-0xdeadbeef" as unknown as number }, "t/r")).not.toThrow();
    expect(() => check(Schema, { a: 42 }, "t/report")).toThrow();
    const report = seen.find((r) => r.boundary === "t/report");
    expect(report?.paths).toEqual(["a"]);
    expect(JSON.stringify(seen)).not.toContain("secret-wallet");
  });

  test("a throwing reporter cannot break the app", () => {
    setValidationReporter(() => {
      throw new Error("reporter is down");
    });
    expect(() => check(Schema, { a: 1 }, "t/badreporter")).toThrow(/validation failed/);
  });

  describe("production branch", () => {
    beforeEach(() => setValidationDevMode(false));

    test("returns the ORIGINAL value rather than a default, and does not throw", () => {
      const value = { a: 1, keepMe: true };
      const out = check(Schema, value, "t/prod") as unknown as typeof value;
      expect(out).toBe(value);
      expect(out.keepMe).toBe(true);
    });

    test("warns once per boundary, then stays quiet", () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      check(Schema, { a: 1 }, "t/noisy");
      check(Schema, { a: 2 }, "t/noisy");
      check(Schema, { a: 3 }, "t/noisy");
      expect(warn).toHaveBeenCalledTimes(1);
      expect(validationFailures().get("t/noisy")).toBe(3);
    });
  });
});
