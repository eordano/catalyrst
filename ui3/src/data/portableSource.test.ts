import { expect, test } from "vitest";
import { portableSource } from "./portableSource";

test("accepts a single world or realm argument, never a console flag or multiple commands", () => {
  expect(portableSource(" Demo.dcl.eth ")).toBe("demo.dcl.eth");
  expect(portableSource("https://worlds.example/world/demo")).toBe("https://worlds.example/world/demo");
  for (const source of ["demo", "--help", "demo.dcl.eth true", "https://example/a\n/kill all", "javascript:alert(1)", "https://user:pass@example/"])
    expect(() => portableSource(source)).toThrow();
});
