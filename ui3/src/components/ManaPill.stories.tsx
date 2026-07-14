import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import ManaPill from "./ManaPill";

const meta = {
  tags: ["autodocs"],
  title: "Components/ManaPill",
  component: ManaPill,
  parameters: { layout: "centered" },
} satisfies Meta<typeof ManaPill>;

export default meta;
type Story = StoryObj<typeof meta>;

const Dark = ({ children }: { children: ReactNode }) => (
  <div style={{ background: "#000", padding: 20, display: "inline-flex" }}>{children}</div>
);

export const Default: Story = { render: () => <Dark><ManaPill value="2,480.55" /></Dark> };
export const Zero: Story = { render: () => <Dark><ManaPill value="0.00" /></Dark> };
