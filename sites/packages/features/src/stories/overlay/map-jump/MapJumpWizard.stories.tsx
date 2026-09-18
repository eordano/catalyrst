import { MemoryRouter } from "react-router";
import { expect, userEvent, waitFor, within, type Meta, type StoryObj } from "@ui/docs/sb";
import { FakeBridge } from "@ui/test/fakeBridge";
import { lifecycleFixture } from "@ui/test/lifecycleFixture";
import type { LifecycleSnapshot } from "@ui/generated/bridge/LifecycleSnapshot";
import type { TravelStatus } from "@ui/generated/bridge/TravelStatus";
import MapJumpWizard from "./MapJumpWizard";

const travel: TravelStatus = {
  operation: { session: "engine-a", realm: 1, instance: 1, requestId: "travel-existing", attempt: 0 },
  realmOperation: null, phase: "preparing", realm: "world.dcl.eth", parcel: [0, 0],
  blockingReason: "Starting destination scene", canCancel: true,
};

function installBridge(initial: LifecycleSnapshot = lifecycleFixture()) {
  const previous = window.dclBridge;
  const bridge = new FakeBridge();
  const send = bridge.send;
  let snapshot = initial;
  bridge.send = (action, payload) => {
    send(action, payload);
    if (action === "Travel") {
      const command = bridge.lastSent("Travel")!;
      snapshot = { ...snapshot, revision: snapshot.revision + 1, travel: { ...travel, operation: { ...travel.operation, requestId: command.requestId } } };
    }
    if (action === "CancelTravel") {
      const command = bridge.lastSent("CancelTravel")!;
      snapshot = { ...snapshot, revision: snapshot.revision + 1, travel: snapshot.travel ? { ...snapshot.travel, phase: "cancelled", canCancel: false } : null,
        commandResults: [{ requestId: command.requestId, action: "cancelTravel", accepted: true, error: null }] };
    }
    if (["GetLifecycleSnapshot", "Travel", "CancelTravel"].includes(action)) bridge.push({ kind: "lifecycle", snapshot });
  };
  window.dclBridge = bridge;
  return () => { window.dclBridge = previous; };
}

const meta = {
  title: "Sites/Overlay/Map travel",
  component: MapJumpWizard,
  parameters: { layout: "fullscreen" },
  args: {
    trackCtx: { sid: "story", story: "map-jump", variant: "engine", experimentKey: "map-jump" },
    track: () => {},
    data: { source: "catalyst", pins: [{ id: "plaza", name: "Genesis Plaza", coords: "0,0", x: 0, y: 0, category: "poi", users: 12, rating: 98, live: false, featured: true, creator: "Decentraland", world: false, worldName: null, image: null }] },
  },
} satisfies Meta<typeof MapJumpWizard>;
export default meta;
type Story = StoryObj<typeof meta>;

export const InProgress: Story = {
  beforeEach: () => installBridge(lifecycleFixture({ travel })),
  render: (args) => <MemoryRouter initialEntries={["/?panel=map&step=jump&select=0,0"]}><MapJumpWizard {...args} /></MemoryRouter>,
};

export const Arrival: Story = {
  beforeEach: () => installBridge(),
  render: (args) => <MemoryRouter initialEntries={["/?panel=map&step=confirm&select=0,0"]}><MapJumpWizard {...args} /></MemoryRouter>,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole("button", { name: "confirm & jump" }));
    await canvas.findByText("Starting destination scene");
    const bridge = window.dclBridge as FakeBridge;
    const request = bridge.lastSent("Travel")!;
    await expect(request).toBeDefined();
    await expect(request).toMatchObject({ realm: "", parcel: [0, 0] });
    await expect(bridge.sentOf("Travel")).toHaveLength(1);
    const operation = { ...travel.operation, requestId: request.requestId };
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 3, travel: { ...travel, operation, phase: "arrived", canCancel: false }, outcomes: [{ operation, phase: "arrived", reason: null }] }) });
    await canvas.findByText("You've arrived");
    await expect(bridge.sentOf("Travel")).toHaveLength(1);
  },
};

export const ReopenedDuringTravel: Story = {
  beforeEach: () => installBridge(lifecycleFixture({ travel })),
  render: (args) => <MemoryRouter initialEntries={["/?panel=map&step=jump&select=0,0"]}><MapJumpWizard {...args} /></MemoryRouter>,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText("Starting destination scene");
    const bridge = window.dclBridge as FakeBridge;
    await expect(bridge.sentOf("Travel")).toHaveLength(0);
    await userEvent.click(canvas.getByRole("button", { name: "Cancel teleport" }));
    await waitFor(() => expect(canvas.queryByText("Teleporting\u2026")).toBeNull());
    await expect(bridge.sentOf("CancelTravel")).toEqual([{ requestId: "travel-existing", expectedSession: "engine-a" }]);
    await expect(bridge.sentOf("Travel")).toHaveLength(0);
  },
};
