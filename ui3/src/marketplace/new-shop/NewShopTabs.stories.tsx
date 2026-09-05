import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fn, userEvent, within } from "storybook/test";
import NewShopTabs, { NEW_SHOP_TABS } from "./NewShopTabs";

const meta = {
  title: "Marketplace/NewShop/Tabs",
  component: NewShopTabs,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  args: { tabs: NEW_SHOP_TABS, active: "overview", onTab: fn() },
} satisfies Meta<typeof NewShopTabs>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const SwitchTab: Story = {
  render: (args) => {
    const [active, setActive] = useState("overview");
    return (
      <NewShopTabs
        {...args}
        active={active}
        onTab={(id) => {
          setActive(id);
          args.onTab?.(id);
        }}
      />
    );
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(canvas.getByRole("tab", { name: "All Assets" }));
    await expect(args.onTab).toHaveBeenCalledWith("all-assets");
    await expect(canvas.getByRole("tab", { name: "All Assets" })).toHaveAttribute("aria-selected", "true");
    await expect(canvas.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "false");
  },
};

export const KeyboardArrows: Story = {
  render: (args) => {
    const [active, setActive] = useState("overview");
    return (
      <NewShopTabs
        {...args}
        active={active}
        onTab={(id) => {
          setActive(id);
          args.onTab?.(id);
        }}
      />
    );
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const overview = canvas.getByRole("tab", { name: "Overview" });
    await expect(overview).toHaveAttribute("tabindex", "0");
    await expect(canvas.getByRole("tab", { name: "All Assets" })).toHaveAttribute("tabindex", "-1");

    overview.focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(args.onTab).toHaveBeenCalledWith("all-assets");
    await expect(canvas.getByRole("tab", { name: "All Assets" })).toHaveAttribute("aria-selected", "true");

    await userEvent.keyboard("{End}");
    await expect(args.onTab).toHaveBeenCalledWith("my-favorites");
    await userEvent.keyboard("{Home}");
    await expect(canvas.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
  },
};
