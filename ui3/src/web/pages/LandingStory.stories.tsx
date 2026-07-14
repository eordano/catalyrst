import type { Meta, StoryObj } from "@storybook/react-vite";
import LandingStory from "./LandingStory";

const meta = {
  title: "Web/Pages/LandingStory",
  component: LandingStory,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof LandingStory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <LandingStory />,
};

export const SceneCreators: Story = {
  render: () => (
    <LandingStory
      audience="Scene & game creators"
      headline="Your game. Your rules. Your people."
      subhead={"Build a 3D world people show up for \u{2014} open, yours, and unkillable."}
      beats={[
        { title: "Press play in seconds", body: "Build in the browser, drop in a cube, and watch your avatar walk your scene." },
        { title: "It's yours, for good", body: "Open-source runtime, portable assets, a stage no platform can pull out from under you." },
        { title: "Distribution that isn't a lottery", body: "Launch into a dense destination where players already are." },
      ]}
      cta={{ label: "Start building \u{2014} free", href: "/create" }}
    />
  ),
};

export const Players: Story = {
  render: () => (
    <LandingStory
      audience="Players & socializers"
      headline="The party already started."
      subhead={"Drop into live events, games, and rooms full of real people \u{2014} no download, just a link."}
      beats={[
        { title: "Something's on right now", body: "Parties, tournaments, openings \u{2014} not an empty map to wander." },
        { title: "Your look is the opener", body: "Your avatar, your style, your expression \u{2014} the fun starts before the game does." },
        { title: "Free to jump in", body: "Play from the browser; when you buy, you pay in plain dollars and actually own it." },
      ]}
      cta={{ label: "Jump in", href: "/discover" }}
    />
  ),
};
