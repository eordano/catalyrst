import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ACTION_SCHEMAS, COMPONENT_SCHEMAS, schemaDefault, schemaError, type AuthoringSchema } from "../authoring-schema";
import { DeSchemaFields } from "./DeSchemaFields";
import DeInteractionsPanel, { ACTIONS, TRIGGERS } from "./DeInteractionsPanel";

afterEach(cleanup);
function Harness({ schema, initial = {} }: { schema: AuthoringSchema; initial?: unknown }) {
  const [value, setValue] = useState(initial);
  return <><DeSchemaFields schema={schema} value={value} onChange={setValue} /><output data-testid="serialized">{JSON.stringify(value)}</output></>;
}
const value = () => JSON.parse(screen.getByTestId("serialized").textContent!);
function change(label: string, next: string) { const input = screen.getByLabelText(label); fireEvent.change(input, { target: { value: next } }); fireEvent.blur(input); }

describe("upstream schema authoring", () => {
  it("exposes every upstream action and trigger through the existing composer", () => {
    expect(ACTIONS.map(action => action.id).sort()).toEqual(Object.keys(ACTION_SCHEMAS).sort());
    const triggerTypes = (COMPONENT_SCHEMAS["asset-packs::Triggers"]!.schema.properties!.value!.items!.properties!.type!.enum ?? []).map(String);
    expect(triggerTypes.length).toBeGreaterThan(0);
    expect(TRIGGERS.map(trigger => trigger.id)).toEqual(expect.arrayContaining(triggerTypes));
  });
  it("edits a real light union variant and serializes the engine field names", () => {
    const schema = COMPONENT_SCHEMAS["core::LightSource"]!.schema;
    render(<Harness schema={schema} />);
    fireEvent.click(screen.getByRole("button", { name: "Set type" }));
    fireEvent.change(screen.getByLabelText("Type"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "Set outer angle" }));
    change("Outer angle", "45");
    expect(value()).toEqual({ type: { spot: { outerAngle: 45 } } });
    expect(schemaError(schema, value())).toBeNull();
  });
  it("edits script parameters by type and uses upstream enum values", () => {
    render(<Harness schema={ACTION_SCHEMAS.call_script_method!.schema} initial={ACTION_SCHEMAS.call_script_method!.defaults} />);
    change("Script path", "src/door.ts");
    change("Method name", "open");
    fireEvent.click(screen.getByRole("button", { name: "Set params" }));
    fireEvent.change(screen.getByLabelText("Params field name"), { target: { value: "speed" } });
    fireEvent.change(screen.getByLabelText("Params field type"), { target: { value: "number" } });
    fireEvent.click(screen.getByRole("button", { name: "Add field" }));
    change("Speed", "2.5");
    expect(value()).toEqual({ scriptPath: "src/door.ts", methodName: "open", params: { speed: 2.5 } });
    expect(schemaError(ACTION_SCHEMAS.call_script_method!.schema, value())).toBeNull();
  });
  it("adds conditions with entity references and exact string comparison values", () => {
    const schema = COMPONENT_SCHEMAS["asset-packs::Triggers"]!.schema.properties!.value!.items!;
    render(<Harness schema={schema} initial={schemaDefault(schema)} />);
    fireEvent.click(screen.getByRole("button", { name: "Set conditions" }));
    fireEvent.click(screen.getByRole("button", { name: "Add conditions" }));
    fireEvent.click(screen.getByRole("button", { name: "Set id" }));
    change("Id", "512");
    change("Value", "3");
    const types = screen.getAllByLabelText("Type");
    fireEvent.change(types[1]!, { target: { value: "when_counter_equals" } });
    expect(value().conditions).toEqual([{ id: 512, type: "when_counter_equals", value: "3" }]);
    expect(schemaError(schema, value())).toBeNull();
  });
  it("composes newly supported counter actions into actual SDK components", async () => {
    const writes: Array<[string, string]> = [];
    render(<DeInteractionsPanel entityId="512" onWrite={(name, json) => { writes.push([name, json]); }} />);
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "set_counter" } });
    change("Counter", "7");
    fireEvent.click(screen.getByRole("button", { name: "Add interaction" }));
    await waitFor(() => expect(writes).toHaveLength(2));
    const action = JSON.parse(writes[0]![1]).value[0];
    expect(action.type).toBe("set_counter");
    expect(JSON.parse(action.jsonPayload)).toEqual({ counter: 7 });
    expect(JSON.parse(writes[1]![1]).value[0].actions[0].id).toBe(512);
  });
  it("rejects malformed nested field values before composing", () => {
    expect(schemaError(ACTION_SCHEMAS.start_tween!.schema, { ...ACTION_SCHEMAS.start_tween!.defaults as object, duration: Infinity })).toMatch(/finite number/);
    expect(schemaError(COMPONENT_SCHEMAS["core::AvatarAttach"]!.schema, { anchorPointId: 999 })).toMatch(/supported value/);
  });
});

it("waits for the engine batch acknowledgment and shows a rejected interaction", async () => {
  let reject!: (error: Error) => void;
  render(<DeInteractionsPanel entityId="512" onWriteBatch={() => new Promise((_resolve, fail) => { reject = fail; })} />);
  fireEvent.click(screen.getByRole("button", { name: "Add interaction" }));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("button", { name: "Adding interaction\u2026" })).toHaveProperty("disabled", true);
  reject(new Error("The scene disconnected"));
  expect((await screen.findByRole("alert")).textContent).toContain("The scene disconnected");
  expect(screen.queryByRole("status")).toBeNull();
});

it("preserves existing interactions and gives appended actions distinct names", async () => {
  const first = { name: ACTIONS[0]!.label, type: ACTIONS[0]!.id, jsonPayload: "{}" };
  const trigger = { type: "on_click", actions: [{ id: 512, name: first.name }] };
  let changes: { name: string; json: string }[] = [];
  render(<DeInteractionsPanel entityId="512" existingActions={{ id: 512, value: [first] }} existingTriggers={{ value: [trigger] }} onWriteBatch={async value => { changes = value; }} />);
  fireEvent.click(screen.getByRole("button", { name: "Add interaction" }));
  await waitFor(() => expect(changes).toHaveLength(2));
  expect(JSON.parse(changes[0]!.json).value[0]).toEqual(first);
  expect(JSON.parse(changes[0]!.json).value[1].name).toBe(`${first.name} 2`);
  expect(JSON.parse(changes[1]!.json).value[0]).toEqual(trigger);
});
