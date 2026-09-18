import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import ChScenes from "./ChScenes";

const meta = {
  title: "CreatorHub/Pages/Scenes",
  component: ChScenes,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChScenes>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { state: "default" },
};

export const Empty: Story = {
  args: { state: "empty" },
};

export const Loading: Story = {
  args: { state: "loading" },
};

export const SceneLists: Story = {
  args: {
    state: "default",
    projects: [{ id: "scene-layout", title: "Tower scene", layout: { cols: 1, rows: 1 } }],
    tabs: [
      { id: "local", label: "Local scenes", active: true },
      { id: "published", label: "Published scenes", active: false },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const tabs = canvas.getByRole("navigation", { name: "Scene lists" });
    const heading = canvas.getByRole("heading", { name: "My Scenes" });
    expect(tabs.getBoundingClientRect().bottom).toBeLessThanOrEqual(heading.getBoundingClientRect().top);
    for (const button of within(tabs).getAllByRole("button")) {
      expect(button.getBoundingClientRect().height).toBeLessThan(60);
    }
  },
};
