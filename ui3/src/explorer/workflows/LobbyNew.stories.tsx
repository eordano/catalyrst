import type { Meta, StoryObj } from "@storybook/react-vite";
import { userEvent, within } from "storybook/test";
import LobbyNew from "./LobbyNew";

const meta = {
  title: "Explorer/Workflows/LobbyNew",
  component: LobbyNew,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof LobbyNew>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <LobbyNew />,
};

export const PickYourName: Story = {
  render: () => <LobbyNew />,
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole("button", { name: "Play as a guest" }));
  },
};
