import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { z } from "zod";

import {
  check,
  resetValidationFailures,
  setValidationDevMode,
  setValidationReporter,
  validationFailures,
} from "../validate";

const Schema = z.object({ a: z.string() });

beforeEach(() => {
  resetValidationFailures();
  setValidationDevMode(true);
});
afterEach(() => {
  resetValidationFailures();
  vi.restoreAllMocks();
});

describe("check", () => {
  test("returns parsed data on success; in dev it throws naming the persisted-state cause and still counts the failure", () => {
    expect(check(Schema, { a: "x" }, "t/ok")).toEqual({ a: "x" });
    expect(validationFailures().size).toBe(0);

    expect(() => check(Schema, { a: 1 }, "t/bad")).toThrow(/validation failed at t\/bad/);
    expect(() => check(Schema, { a: 1 }, "t/bad")).toThrow(/older build/);
    expect(validationFailures().get("t/bad")).toBe(2);
  });

  test("reports paths and never the value, and a throwing reporter cannot break the app", () => {
    const seen: { boundary: string; detail: string; paths: string[] }[] = [];
    setValidationReporter((r) => seen.push(r));
    expect(() => check(Schema, { a: "secret-wallet-0xdeadbeef" as unknown as number }, "t/r")).not.toThrow();
    expect(() => check(Schema, { a: 42 }, "t/report")).toThrow();
    const report = seen.find((r) => r.boundary === "t/report");
    expect(report?.paths).toEqual(["a"]);
    expect(JSON.stringify(seen)).not.toContain("secret-wallet");

    setValidationReporter(() => {
      throw new Error("reporter is down");
    });
    expect(() => check(Schema, { a: 1 }, "t/badreporter")).toThrow(/validation failed/);
  });

  test("in production it returns the ORIGINAL value without throwing and warns once per boundary", () => {
    setValidationDevMode(false);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const value = { a: 1, keepMe: true };
    const out = check(Schema, value, "t/prod") as unknown as typeof value;
    expect(out).toBe(value);
    expect(out.keepMe).toBe(true);

    check(Schema, { a: 2 }, "t/prod");
    check(Schema, { a: 3 }, "t/prod");
    expect(warn).toHaveBeenCalledTimes(1);
    expect(validationFailures().get("t/prod")).toBe(3);
  });
});
