import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import DeSceneSettings from "./DeSceneSettings";
afterEach(cleanup);

it("acknowledges persisted settings and keeps edits visible after a rejected save", async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error("Changed outside this form")).mockResolvedValue(undefined);
  render(<DeSceneSettings load={async () => ({ content: '{"display":{"title":"Before","custom":42}}', destination: "SDK project", save })} onClose={() => {}} />);
  const name = await screen.findByLabelText("Scene name");
  fireEvent.change(name, { target: { value: "After" } });
  fireEvent.click(screen.getByText("Save settings"));
  expect(await screen.findByRole("alert")).toHaveTextContent("Changed outside");
  expect(name).toHaveValue("After");
  fireEvent.click(screen.getByText("Save settings"));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saved to sdk project"));
  expect(JSON.parse(save.mock.calls[1]![0])).toEqual({ display: { title: "After", custom: 42 } });
});

it("saves multiple spawn areas, reassigns the deleted default and reloads persisted fields", async () => {
  let content = JSON.stringify({ display: { title: "Beach" }, spawnPoints: [{ name: "dock", default: true, position: { x: 2, y: 0, z: 2 }, extension: "kept" }] });
  const save = vi.fn(async (next: string) => { content = next; });
  const load = async () => ({ content, destination: "SDK project", save });
  const view = render(<DeSceneSettings load={load} onClose={() => {}} />);
  await screen.findByLabelText("Scene name");
  fireEvent.click(screen.getByText("Parcels and spawn areas"));
  fireEvent.click(screen.getByText("Duplicate spawn area"));
  fireEvent.change(screen.getByLabelText("Spawn area name"), { target: { value: "garden" } });
  fireEvent.change(screen.getByLabelText("Spawn X"), { target: { value: "4, 6" } });
  fireEvent.change(screen.getByLabelText("Spawn area"), { target: { value: "0" } });
  fireEvent.click(screen.getByText("Delete spawn area"));
  expect(screen.getByLabelText("Default spawn area")).toBeChecked();
  fireEvent.click(screen.getByText("Save settings"));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saved"));
  expect(JSON.parse(content).spawnPoints).toEqual([{ name: "garden", default: true, position: { x: [4, 6], y: [0, 0], z: [2, 2] }, extension: "kept" }]);
  view.unmount();
  render(<DeSceneSettings load={load} onClose={() => {}} />);
  expect(await screen.findByLabelText("Spawn area name")).toHaveValue("garden");
  expect(screen.getByLabelText("Spawn X")).toHaveValue("4, 6");
});
