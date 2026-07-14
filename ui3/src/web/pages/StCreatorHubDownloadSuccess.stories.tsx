import type { Meta, StoryObj } from "@storybook/react-vite";
import StCreatorHubDownloadSuccess from "./StCreatorHubDownloadSuccess";

const meta = {
  title: "Web/Pages/Download/Creator Hub Success",
  component: StCreatorHubDownloadSuccess,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StCreatorHubDownloadSuccess>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { os: "macos", loading: false },
};

export const Windows: Story = {
  args: { os: "windows", loading: false },
};

export const Loading: Story = {
  args: { os: "macos", loading: true },
};
