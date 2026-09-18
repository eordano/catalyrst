import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { applyDeviceEntries, emptyDeviceTelemetry } from "../device-debug";
import DeDeviceTelemetry from "./DeDeviceTelemetry";
afterEach(cleanup);
it("shows measured performance and scene-scoped component details without inventing missing metrics", () => {
  const data = applyDeviceEntries(emptyDeviceTelemetry(), [
    { type: "perf", fps: 60, mem_gpu_mb: 128 }, { type: "perf", fps: 59, mem_gpu_mb: 130 },
    { type: "crdt", sid: 1, e: 4, c: "Name", op: "p", payload: { value: "Beach" } },
    { type: "crdt", sid: 2, e: 4, c: "Name", op: "p", payload: { value: "Forest" } },
  ]);
  render(<DeDeviceTelemetry data={data} />);
  expect(screen.getByText("59")).toBeInTheDocument();
  expect(screen.getByText("130 MB")).toBeInTheDocument();
  expect(screen.getAllByText("Not reported").length).toBeGreaterThan(0);
  expect(screen.getByRole("img", { name: "FPS history, last 2 samples" })).toBeInTheDocument();
  fireEvent.click(screen.getByText("Entities and components"));
  fireEvent.change(screen.getByLabelText("Scene"), { target: { value: "2" } });
  expect(screen.queryByRole("button", { name: "Scene 1 \u00b7 Entity 4" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Scene 2 \u00b7 Entity 4" }));
  fireEvent.click(screen.getByText("Name"));
  expect(screen.getByText(/Forest/)).toBeInTheDocument();
  expect(screen.queryByText(/Beach/)).not.toBeInTheDocument();
});
it("clears disposed-scene filters and selection instead of keeping a stale inspector", () => {
  const data = applyDeviceEntries(emptyDeviceTelemetry(), [
    { type: "crdt", sid: 1, e: 4, c: "Name", op: "p", payload: { value: "Beach" } },
    { type: "crdt", sid: 2, e: 4, c: "Name", op: "p", payload: { value: "Forest" } },
  ]);
  const { rerender } = render(<DeDeviceTelemetry data={data} />);
  fireEvent.click(screen.getByText("Entities and components"));
  fireEvent.change(screen.getByLabelText("Scene"), { target: { value: "1" } });
  fireEvent.click(screen.getByRole("button", { name: "Scene 1 \u00b7 Entity 4" }));
  rerender(<DeDeviceTelemetry data={applyDeviceEntries(data, [{ type: "scene_lifecycle", event: "scene_dispose", scene_id: 1 }])} />);
  expect(screen.getByLabelText("Scene")).toHaveValue("");
  expect(screen.getByRole("button", { name: "Scene 2 \u00b7 Entity 4" })).toBeInTheDocument();
  expect(screen.getByText(/Select an entity/)).toBeInTheDocument();
});
it("bounds the rendered entity list while keeping filtered entities discoverable", () => {
  const data = applyDeviceEntries(emptyDeviceTelemetry(), Array.from({ length: 250 }, (_, e) => ({ type: "crdt", sid: 1, e, c: "Name", op: "p", payload: { value: String(e) } })));
  render(<DeDeviceTelemetry data={data} />);
  fireEvent.click(screen.getByText("Entities and components"));
  expect(screen.getAllByRole("listitem")).toHaveLength(200);
  expect(screen.getByRole("status")).toHaveTextContent("first 200 of 250");
  fireEvent.change(screen.getByLabelText("Find entity or component"), { target: { value: "249" } });
  expect(screen.getByRole("button", { name: "Scene 1 \u00b7 Entity 249" })).toBeInTheDocument();
});
