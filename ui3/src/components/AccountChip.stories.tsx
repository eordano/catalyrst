import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import AccountChip from "./AccountChip";

const meta = {
  tags: ["autodocs"],
  title: "Components/AccountChip",
  component: AccountChip,
  parameters: { layout: "centered" },
} satisfies Meta<typeof AccountChip>;

export default meta;
type Story = StoryObj<typeof meta>;

const Dark = ({ children }: { children: ReactNode }) => (
  <div style={{ background: "#000", padding: 20, display: "inline-flex" }}>{children}</div>
);

export const Default: Story = {
  render: () => (
    <Dark>
      <AccountChip account={"0x9f3c\u{2026}7a21"} />
    </Dark>
  ),
};
