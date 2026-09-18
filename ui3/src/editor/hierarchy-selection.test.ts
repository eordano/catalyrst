import { describe, expect, it } from "vitest";
import { hierarchySelection } from "./hierarchy-selection";
describe("hierarchy selection", () => {
  const visible = ["0", "512", "513", "514"];
  it("toggles Ctrl/Meta selections and selects a visible Shift range", () => {
    expect(hierarchySelection(visible, ["512"], "512", "514", { ctrlKey: true })).toEqual(["512", "514"]);
    expect(hierarchySelection(visible, ["512", "514"], "514", "512", { metaKey: true })).toEqual(["514"]);
    expect(hierarchySelection(visible, ["514"], "514", "512", { shiftKey: true })).toEqual(["512", "513", "514"]);
  });
  it("replaces the selection when the anchor is hidden", () => {
    expect(hierarchySelection(["0", "514"], ["512"], "512", "514", { shiftKey: true })).toEqual(["514"]);
  });
});
