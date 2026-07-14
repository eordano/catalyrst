import type { ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Avatar, StatusDot, Badge, Pill, WalletChip, ProgressBar, Skeleton, Tooltip } from "./primitives";

const meta = {
  title: "Atoms/Primitives",
  component: Pill,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    variant: { control: "select", options: ["default", "accent", "live", "online"] },
    children: { control: "text" },
  },
  args: { variant: "accent", children: "ACCENT" },
} satisfies Meta<typeof Pill>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const Row = ({ children }: { children?: ReactNode }) => (
  <div style={{ display: "flex", gap: 16, alignItems: "center", flexWrap: "wrap" }}>{children}</div>
);

export const Catalog: Story = {
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 20, width: 320 }}>
      <Row>
        <Avatar hue={280} size={48} />
        <Avatar hue={20} size={48} status="online" />
        <Avatar hue={200} size={48} status="away" />
        <Avatar hue={320} size={48} status="offline" />
      </Row>
      <Row>
        <StatusDot status="online" /><StatusDot status="away" /><StatusDot status="offline" />
        <Badge>1</Badge><Badge>9</Badge><Badge>99+</Badge>
      </Row>
      <Row>
        <Pill variant="default">DEFAULT</Pill>
        <Pill variant="accent">ACCENT</Pill>
        <Pill variant="live">LIVE</Pill>
        <Pill variant="online">ONLINE</Pill>
      </Row>
      <Row><WalletChip address={"0x10e\u{2026}a7a92"} /></Row>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <ProgressBar value={35} />
        <ProgressBar value={80} />
        <ProgressBar value={95} danger />
      </div>
      <Skeleton lines={3} width={240} />
      <Tooltip label="Hello from the design system"><Pill variant="accent">hover me</Pill></Tooltip>
    </div>
  ),
};
