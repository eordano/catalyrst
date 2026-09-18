import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { AxisRow } from "./DeInspectorFields";

test("keeps transform fields controlled while nudge availability changes", () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const value = { x: 1, y: 2, z: 3 };
  const { rerender } = render(<AxisRow label="position" v={value} />);
  rerender(<AxisRow label="position" v={value} onNudge={() => {}} />);
  rerender(<AxisRow label="position" v={value} />);

  expect(screen.getByLabelText("position X")).toHaveValue("1");
  expect(error).not.toHaveBeenCalledWith(expect.stringContaining("controlled input"));
  error.mockRestore();
});
