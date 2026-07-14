import type { Meta, StoryObj } from "@storybook/react-vite";
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
