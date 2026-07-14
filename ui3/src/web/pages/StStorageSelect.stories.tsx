import type { Meta, StoryObj } from "@storybook/react-vite";
import StStorageSelect from "./StStorageSelect";
import type { Land, World } from "./StStorageSelect";

const WORLDS: World[] = [
  { name: "genesis.dcl.eth", role: "owner", scenes: 1 },
  { name: "myestate.dcl.eth", role: "owner", scenes: 3 },
  { name: "gallery.dcl.eth", role: "collaborator", scenes: 1 },
  { name: "lounge.dcl.eth", role: "owner", scenes: 2 },
  { name: "studio.dcl.eth", role: "collaborator", scenes: 0 },
];

const LANDS: Land[] = [
  { id: "0x1", name: "Casa Verde", role: "Owner" },
  { id: "0x2", name: "Plaza Norte (12 parcels)", role: "Owner" },
  { id: "0x3", name: "Parcel 14,-9", role: "Operator" },
  { id: "0x4", name: "Beachfront Estate", role: "Tenant" },
  { id: "0x5", name: "Downtown Lot", role: "Lessor" },
  { id: "0x6", name: "Parcel -33,52", role: "Owner" },
];

const meta = {
  title: "Web/Pages/Storage/Select",
  component: StStorageSelect,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StStorageSelect>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <StStorageSelect worlds={WORLDS} lands={LANDS} />,
};

export const Loading: Story = {
  render: () => <StStorageSelect loading />,
};

export const Empty: Story = {
  render: () => <StStorageSelect worlds={[]} lands={[]} />,
};
