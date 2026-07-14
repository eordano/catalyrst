import type { Meta, StoryObj } from "@storybook/react-vite";
import PlaceDetail from "./PlaceDetail";

const meta = {
  title: "Explorer/Pages/PlaceDetail",
  component: PlaceDetail,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof PlaceDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <PlaceDetail />,
};
