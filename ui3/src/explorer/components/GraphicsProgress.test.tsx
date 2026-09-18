import { act, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import GraphicsProgress from "./GraphicsProgress";

test("graphics feedback stays until two engine frames and restarts for another change", () => {
  render(<GraphicsProgress />);
  const emit = (name: string) => act(() => { window.dispatchEvent(new Event(name)); });
  emit("dcl-graphics-change");
  expect(screen.getByRole("status")).toHaveTextContent("Applying graphics settings");
  emit("dcl-engine-frame");
  expect(screen.getByRole("status")).toBeInTheDocument();
  emit("dcl-graphics-change");
  emit("dcl-engine-frame");
  expect(screen.getByRole("status")).toBeInTheDocument();
  emit("dcl-engine-frame");
  expect(screen.queryByRole("status")).toBeNull();
});
