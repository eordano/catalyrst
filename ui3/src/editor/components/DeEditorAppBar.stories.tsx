import { siteUrl } from "../../data/site";
import type { Meta, StoryObj } from "@storybook/react-vite";
import DeEditorAppBar, { DeEditorControlsBar } from "./DeEditorAppBar";

const meta = {
  title: "Editor/Components/Editor App Bar",
  component: DeEditorAppBar,
  args: { title: "Genesis Plaza" },
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DeEditorAppBar>;
export default meta;

type Story = StoryObj<typeof meta>;

const publishOptions = [
  { id: "publish-scene", label: "Publish Scene" },
  { id: "republish", label: "Republish to my-world.dcl.eth" },
];

export const Online: Story = {
  render: () => (
    <DeEditorAppBar
      title="Genesis Plaza"
      viewportSrc={siteUrl("/play")}
      publishOptions={publishOptions}
      onExit={() => {}}
      onPublish={() => {}}
    />
  ),
};

export const PreviewOnly: Story = {
  render: () => (
    <DeEditorAppBar
      title="Untitled scene"
      publishOptions={publishOptions}
      onExit={() => {}}
      onPublish={() => {}}
    />
  ),
};

export const ControlsBar: Story = {
  render: () => (
    <DeEditorControlsBar label='Editing "Genesis Plaza"'>
      <button type="button" className="editor-wizard__btn editor-wizard__btn--primary">
        Open Assets
      </button>
      <button type="button" className="editor-wizard__btn">
        Save to disk
      </button>
    </DeEditorControlsBar>
  ),
};
