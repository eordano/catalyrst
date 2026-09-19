import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import ComponentClipboardControls from "./ComponentClipboardControls";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("waits for an acknowledged copy and pastes upstream payload through the engine", async () => {
  const payload = { __dclComponent: "core::Material", value: { material: { metallic: 0.2 } } };
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText, readText: async () => JSON.stringify(payload) } });
  const actions = { copy: vi.fn().mockResolvedValue(payload), paste: vi.fn().mockResolvedValue(["4", "5"]) };
  render(<ComponentClipboardControls entity="4" name="Material" actions={actions} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy Material component" }));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith(JSON.stringify(payload)));
  fireEvent.click(screen.getByRole("button", { name: "Paste Material component to selection" }));
  await waitFor(() => expect(actions.paste).toHaveBeenCalledWith("Material", payload));
  expect(await screen.findByRole("status")).toHaveTextContent("pasted to the selection");
});

it("rejects mismatched components and reports engine write failures", async () => {
  const readText = vi.fn().mockResolvedValue(JSON.stringify({ __dclComponent: "core::Transform", value: {} }));
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { readText } });
  const actions = { copy: vi.fn(), paste: vi.fn().mockRejectedValue(new Error("Scene disconnected")) };
  render(<ComponentClipboardControls entity="4" name="Material" actions={actions} />);
  fireEvent.click(screen.getByRole("button", { name: "Paste Material component to selection" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Copy a Material");
  expect(actions.paste).not.toHaveBeenCalled();
  readText.mockResolvedValue(JSON.stringify({ __dclComponent: "core::Material", value: {} }));
  fireEvent.click(screen.getByRole("button", { name: "Paste Material component to selection" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Scene disconnected");
});
