import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DeCustomItems } from "./DeCustomItems";
import { customItemPayload } from "../custom-items";

afterEach(cleanup);
const composite = JSON.stringify({ version: 1, components: [{ name: "core::Transform", data: { "512": { json: { parent: 0 } } } }] });
it("saves a selected subtree to project files, reopens its shelf, and places it through the engine callback", async () => {
  const files: Record<string, string> = {};
  const code = { hydrate: async () => files, persist: async (path: string, content: string) => { files[path] = content; } };
  const place = vi.fn(async () => {});
  const first = render(<DeCustomItems code={code} selectionName="Palm" onCapture={async () => customItemPayload(composite)} onPlace={place} />);
  fireEvent.click(screen.getByRole("button", { name: "Save selection as custom item" }));
  expect(await screen.findByRole("button", { name: "Place palm" })).toBeTruthy();
  expect(files["assets/custom-items/palm.composite"]).toBe(composite);
  first.unmount();
  render(<DeCustomItems code={code} onPlace={place} />);
  fireEvent.click(await screen.findByRole("button", { name: "Place palm" }));
  expect(await screen.findByRole("status")).toHaveProperty("textContent", "palm placed in the scene.");
  expect(place).toHaveBeenCalledWith(customItemPayload(composite));
});
it("does not announce a saved item when the project write fails", async () => {
  render(<DeCustomItems code={{ hydrate: async () => ({}), persist: async () => { throw new Error("Project write failed"); } }} onCapture={async () => customItemPayload(composite)} />);
  fireEvent.click(screen.getByRole("button", { name: "Save selection as custom item" }));
  expect((await screen.findByRole("alert")).textContent).toBe("Project write failed");
  expect(screen.queryByRole("status")).toBeNull();
});
