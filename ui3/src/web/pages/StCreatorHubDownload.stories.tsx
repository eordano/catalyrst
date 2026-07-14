import type { Meta, StoryObj } from "@storybook/react-vite";
import StCreatorHubDownload, { ICON } from "./StCreatorHubDownload";

const WINDOWS_ICON = ICON.Windows;
const MACOS_ICON = ICON.macOS;

const REL = "https://github.com/decentraland/creator-hub/releases/latest";

const meta = {
  title: "Web/Pages/Download/Creator Hub",
  component: StCreatorHubDownload,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StCreatorHubDownload>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WindowsPrimary: Story = {
  args: {
    primaryOption: { text: "Windows", image: WINDOWS_ICON, link: REL, arch: "amd64" },
    secondaryOptions: [{ text: "macOS", image: MACOS_ICON, link: REL, arch: "arm64" }],
  },
};

export const OnlyPrimary: Story = {
  args: {
    primaryOption: { text: "macOS", image: MACOS_ICON, link: REL, arch: "arm64" },
    secondaryOptions: [],
  },
};
