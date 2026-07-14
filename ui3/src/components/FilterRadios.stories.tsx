import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { useState } from "react";
import FilterRadios from "./FilterRadios";

const meta = {
  tags: ["autodocs"],
  title: "Components/FilterRadios",
  component: FilterRadios,
  parameters: { layout: "centered" },
} satisfies Meta<typeof FilterRadios>;

export default meta;
type Story = StoryObj<typeof meta>;

const Panel = ({ children }: { children: ReactNode }) => (
  <div style={{ width: 240, background: "var(--panel)", border: "1px solid var(--line)", borderRadius: "var(--r-panel)", padding: 16 }}>
    {children}
  </div>
);

const OPTIONS = [
  { id: "all", label: "All" },
  { id: "sale", label: "On Sale" },
  { id: "notforsale", label: "Not for sale" },
];

export const Default: Story = {
  render: () => {
    const [value, setValue] = useState("all");
    return (
      <Panel>
        <FilterRadios name="story-status" value={value} onChange={setValue} options={OPTIONS} />
      </Panel>
    );
  },
};
