import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DeAssetCleanup } from "./DeAssetCleanup";

afterEach(cleanup);
it("requires selected candidates, rechecks references, and deletes the reviewed file revision", async () => {
  const remove = vi.fn(async () => {});
  const project = { id: "test", list: async () => ["src/index.ts"], read: async () => "", write: async () => {}, assets: {
    list: async () => [{ path: "assets/unused.png", size: 100 }], revision: async () => '"old-revision"',
    read: async () => ({ content: new ArrayBuffer(0), revision: '"new-revision"' }), write: async () => {}, remove,
  } };
  render(<DeAssetCleanup project={project} exportComposite={async () => "{}"} />);
  fireEvent.click(screen.getByRole("button", { name: "Review unused files" }));
  const checkbox = await screen.findByRole("checkbox");
  expect(remove).not.toHaveBeenCalled();
  fireEvent.click(checkbox);
  fireEvent.click(screen.getByRole("button", { name: "Remove selected files (1)" }));
  await waitFor(() => expect(remove).toHaveBeenCalledWith("assets/unused.png", '"old-revision"'));
});
it("keeps a file that gained a scene reference after review", async () => {
  const remove = vi.fn(async () => {});
  let composite = "{}";
  const project = { id: "test", list: async () => [], read: async () => "", write: async () => {}, assets: {
    list: async () => [{ path: "assets/unused.png", size: 100 }], revision: async () => "v1", read: async () => ({ content: new ArrayBuffer(0), revision: "v1" }), write: async () => {}, remove,
  } };
  render(<DeAssetCleanup project={project} exportComposite={async () => composite} />);
  fireEvent.click(screen.getByRole("button", { name: "Review unused files" }));
  fireEvent.click(await screen.findByRole("checkbox"));
  composite = '{"src":"assets/unused.png"}';
  fireEvent.click(screen.getByRole("button", { name: "Remove selected files (1)" }));
  expect((await screen.findByRole("alert")).textContent).toContain("now referenced");
  expect(remove).not.toHaveBeenCalled();
});
