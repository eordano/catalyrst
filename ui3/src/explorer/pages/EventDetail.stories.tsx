import type { Meta, StoryObj } from "@storybook/react-vite";
import EventDetail from "./EventDetail";

const meta = {
  title: "Explorer/Pages/EventDetail",
  component: EventDetail,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof EventDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <EventDetail />,
};
