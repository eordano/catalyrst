import type { Meta, StoryObj } from "@storybook/react-vite";
import CreatorHubChrome from "./CreatorHubChrome";

const meta = {
  title: "CreatorHub/Frames/CreatorHubChrome",
  component: CreatorHubChrome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof CreatorHubChrome>;

export default meta;
type Story = StoryObj<typeof meta>;

const Body = () => (
  <div style={{ padding: 40, color: "rgba(255,255,255,.55)", fontSize: 14 }}>
    Page body renders here.
  </div>
);

export const Default: Story = {
  render: () => (
    <CreatorHubChrome active="home">
      <Body />
    </CreatorHubChrome>
  ),
};

export const SignedIn: Story = {
  render: () => (
    <CreatorHubChrome active="home" signedIn>
      <Body />
    </CreatorHubChrome>
  ),
};

export const Committee: Story = {
  render: () => (
    <CreatorHubChrome active="curate" committee>
      <Body />
    </CreatorHubChrome>
  ),
};
