import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import Friends from "./Friends";

const REQ_ADDRESS = "0xReq0000000000000000000000000000000000aa";
const RECEIVED = [
  { name: "Alice", tag: "#1234", date: "2d", hue: 200, address: REQ_ADDRESS },
];

const meta = {
  title: "Explorer/Pages/Friends/Interactions",
  component: Friends,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Friends>;
export default meta;

type Story = StoryObj<typeof meta>;

export const AcceptRequest: Story = {
  args: {
    initialSection: "requests",
    received: RECEIVED,
    friends: [],
    sent: [],
    blocked: [],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("Alice")).toBeInTheDocument();
    await userEvent.click(canvas.getByRole("button", { name: "Accept" }));
    await expect(canvas.queryByText("Alice")).not.toBeInTheDocument();
  },
};
