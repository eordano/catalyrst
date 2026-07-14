import type { Meta, StoryObj } from "@storybook/react-vite";
import ChPublishWizardPublishToWorld from "./ChPublishWizardPublishToWorld";

const PROJECT = {
  title: "Neon Night Market",
  size: "2x2",
  grad: "linear-gradient(135deg, #ff2d55 0%, #350447 100%)",
};
const OWNER = {
  network: "Mainnet",
  address: "0x9f3c\u{2026}7a21",
  username: "javier.dcl",
  verified: true,
  role: "Owner",
};
const SELECTION_DATA = {
  name: "mystore.dcl.eth",
  names: ["mystore.dcl.eth", "gallery.dcl.eth", "lounge.dcl.eth"],
  world: {
    title: "My Store World",
    scenes: 3,
    size: "4x4",
    grad: "linear-gradient(135deg, #438fff 0%, #2f004d 100%)",
  },
};

const meta = {
  title: "CreatorHub/Workflows/Publish to World",
  component: ChPublishWizardPublishToWorld,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChPublishWizardPublishToWorld>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <ChPublishWizardPublishToWorld
      state="selection"
      project={PROJECT}
      owner={OWNER}
      names={SELECTION_DATA.names}
      selectedName={SELECTION_DATA.name}
      world={SELECTION_DATA.world}
    />
  ),
};

export const Empty: Story = {
  render: () => <ChPublishWizardPublishToWorld state="empty" />,
};
