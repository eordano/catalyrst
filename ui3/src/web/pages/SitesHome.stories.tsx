import type { Meta, StoryObj } from "@storybook/react-vite";
import SitesHome from "./SitesHome";

const meta = {
  title: "Web/Pages/Home",
  component: SitesHome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SitesHome>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <SitesHome />,
};
