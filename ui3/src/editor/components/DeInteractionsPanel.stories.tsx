import type { ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import DclEditorChrome from "../frames/DclEditorChrome";
import DeInteractionsPanel from "./DeInteractionsPanel";

const meta = {
  title: "Editor/Components/InteractionsPanel",
  component: DeInteractionsPanel,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DeInteractionsPanel>;
export default meta;

type Story = StoryObj<typeof meta>;

function Frame({ children }: { children?: ReactNode }) {
  return (
    <DclEditorChrome>
      <div className="eui-panel eui-right">
        <div className="eui-panel-head">
          <div className="eui-head-text">
            <span className="eui-overline">Inspector</span>
            <span className="eui-title">Display Cube</span>
          </div>
          <span className="eui-id-badge">#520</span>
        </div>
        <div className="eui-panel-body">{children}</div>
      </div>
    </DclEditorChrome>
  );
}

export const Default: Story = {
  render: () => (
    <Frame>
      <DeInteractionsPanel
        entityId="520"
        entityName="Display Cube"
        onWrite={(name, json) => console.log("[author]", name, json)}
      />
    </Frame>
  ),
};

export const PreviewOnly: Story = {
  render: () => (
    <Frame>
      <DeInteractionsPanel entityId="520" entityName="Display Cube" />
    </Frame>
  ),
};

export const AppendToExisting: Story = {
  render: () => (
    <Frame>
      <DeInteractionsPanel
        entityId="520"
        entityName="Display Cube"
        onWrite={(name, json) => console.log("[author]", name, json)}
        existingActions={{
          id: 520,
          value: [
            { name: "Open", type: "set_visibility", jsonPayload: JSON.stringify({ visible: true }) },
          ],
        }}
        existingTriggers={{
          value: [{ type: "on_click", actions: [{ id: 520, name: "Open" }] }],
        }}
      />
    </Frame>
  ),
};
