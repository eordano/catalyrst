import type { Meta, StoryObj } from "@storybook/react-vite";
import SitesChrome from "./SitesChrome";

const meta = {
  title: "Web/Frames/LandingChrome",
  component: SitesChrome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SitesChrome>;

export default meta;
type Story = StoryObj<typeof meta>;

const Body = () => (
  <div
    style={{
      minHeight: "60vh",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      color: "var(--ink-6)",
      font: "600 15px/1.5 Inter, sans-serif",
      background:
        "linear-gradient(160deg, #45106a 0%, #39055c 50%, #220040 100%)",
    }}
  >
    Page content renders here
  </div>
);

export const Default: Story = {
  render: () => (
    <SitesChrome active="play">
      <Body />
    </SitesChrome>
  ),
};

export const SignedIn: Story = {
  render: () => (
    <SitesChrome active="play" signedIn>
      <Body />
    </SitesChrome>
  ),
};
