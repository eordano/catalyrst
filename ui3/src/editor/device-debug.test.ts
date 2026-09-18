import { expect, it } from "vitest";
import { applyDeviceEntries, deviceEntityDepth, emptyDeviceTelemetry } from "./device-debug";

const put = (sid: number, e: number, c: string, payload: unknown) => ({ type: "crdt", sid, e, c, op: "p", payload });
it("keeps equal entity IDs in different scenes separate and applies deletions without mutating old snapshots", () => {
  const before = applyDeviceEntries(emptyDeviceTelemetry(), [put(1, 4, "Transform", { parent: 3 }), put(2, 4, "Transform", { parent: 8 }), put(1, 4, "Name", { value: "box" })]);
  const removed = applyDeviceEntries(before, [{ ...put(1, 4, "Transform", null), op: "d" }]);
  expect(removed.entities["1:4"]?.parent).toBe(0);
  expect(removed.entities["1:4"]?.components.Transform).toBeUndefined();
  expect(before.entities["1:4"]?.components.Transform).toEqual({ parent: 3 });
  expect(removed.entities["2:4"]?.parent).toBe(8);
  expect(applyDeviceEntries(removed, [{ ...put(1, 4, "", null), op: "de" }]).entities["1:4"]).toBeUndefined();
});
it("preserves missing performance values and bounds FPS and growing event histories", () => {
  const data = applyDeviceEntries(emptyDeviceTelemetry(), Array.from({ length: 105 }, (_, index) => [
    { type: "perf", fps: index, js_heap_used_mb: NaN },
    { ...put(1, 4, "Events", index), op: "a" },
  ]).flat());
  expect(data.fps.length).toBeLessThan(105);
  expect(data.fps.at(-1)).toBe(104);
  expect(data.performance?.draw_calls).toBeUndefined();
  expect(data.performance?.js_heap_used_mb).toBeUndefined();
  expect((data.entities["1:4"]?.components.Events as unknown[]).length).toBeLessThan(105);
});
it("ignores malformed telemetry and terminates cyclic hierarchy walks", () => {
  const data = applyDeviceEntries(emptyDeviceTelemetry(), [null, {}, put(-1, 4, "Name", "bad"), put(1, 4, "Transform", { parent: 5 }), put(1, 5, "Transform", { parent: 4 })]);
  expect(Object.keys(data.entities)).toEqual(["1:4", "1:5"]);
  expect(deviceEntityDepth(data.entities["1:4"]!, data.entities)).toBe(1);
  expect(applyDeviceEntries(data, [{ type: "scene_lifecycle", event: "scene_dispose", scene_id: 1 }]).entities).toEqual({});
});
it("does not resurrect deleted entities from component removals and accepts entity deletion without a component", () => {
  const before = applyDeviceEntries(emptyDeviceTelemetry(), [put(1, 4, "Name", { value: "box" })]);
  const removed = applyDeviceEntries(before, [{ type: "crdt", sid: 1, e: 4, op: "de" }, { ...put(1, 4, "Name", null), op: "d" }]);
  expect(removed.entities).toEqual({});
  expect(before.entities["1:4"]?.components.Name).toEqual({ value: "box" });
});
it("tracks upstream UI parent fields and clears a scene incarnation independently", () => {
  const initial = applyDeviceEntries(emptyDeviceTelemetry(), [put(1, 4, "UiTransform", { parent_entity: 5 }), put(2, 4, "UiTransform", { parent: 9 })]);
  expect(initial.entities["1:4"]?.parent).toBe(5);
  expect(initial.entities["2:4"]?.parent).toBe(9);
  const next = applyDeviceEntries(initial, [{ type: "scene_lifecycle", event: "scene_init", scene_id: 1 }, put(1, 7, "Name", { value: "fresh" })]);
  expect(next.entities["1:4"]).toBeUndefined();
  expect(next.entities["2:4"]?.parent).toBe(9);
  expect(next.entities["1:7"]?.components.Name).toEqual({ value: "fresh" });
});
it("ignores prototype component keys and keeps batched component updates immutable", () => {
  const initial = applyDeviceEntries(emptyDeviceTelemetry(), [put(1, 4, "Name", { value: "original" })]);
  const next = applyDeviceEntries(initial, [put(1, 4, "__proto__", { hidden: true }), put(1, 4, "constructor", {}), put(1, 4, "Name", { value: "changed" }), put(1, 4, "Transform", { parent: 9 })]);
  expect(Object.getPrototypeOf(next.entities["1:4"]!.components)).toBe(Object.prototype);
  expect(Object.keys(next.entities["1:4"]!.components)).toEqual(["Name", "Transform"]);
  expect(initial.entities["1:4"]!.components).toEqual({ Name: { value: "original" } });
});
