import type { ComponentType } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Coin,
  PersonIcon,
  ManaIcon,
  Mute,
  Check,
  Bag,
  Pin,
  CameraIcon,
  People,
  Help,
  ChevronDown,
  ChevronUp,
  ChevronLeft,
  ChevronRight,
  ChevronDownAlt,
  Heart,
  Close,
  Caret,
  Section,
  Search,
  Plus,
  Trash,
  Pencil,
  Gear,
  Copy,
  Info,
  CheckFill,
  CheckBold,
  TriangleDown,
  Kebab,
  GridView,
  ListView,
} from "./icons";

type IconRenderProps = {
  size?: number;
  ring?: boolean;
  open?: boolean;
  compact?: boolean;
  className?: string;
};

const ICONS: Record<string, ComponentType<IconRenderProps>> = {
  Coin,
  ManaIcon,
  PersonIcon,
  Mute,
  Check,
  Bag,
  Pin,
  CameraIcon,
  People,
  Help,
  ChevronDown,
  ChevronUp,
  ChevronLeft,
  ChevronRight,
  ChevronDownAlt,
  Heart,
  Close,
  Caret,
  Section,
  Search,
  Plus,
  Trash,
  Pencil,
  Gear,
  Copy,
  Info,
  CheckFill,
  CheckBold,
  TriangleDown,
  Kebab,
  GridView,
  ListView,
};

type IconStoryArgs = IconRenderProps & { icon: string };

const meta = {
  title: "Atoms/icons",
  component: Coin,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    icon: { control: "select", options: Object.keys(ICONS) },
    size: { control: { type: "range", min: 12, max: 72, step: 2 } },
    ring: { control: "boolean" },
    open: { control: "boolean" },
    compact: { control: "boolean" },
  },
  args: { icon: "Coin", size: 28, ring: true, open: false, compact: false },
} satisfies Meta<IconStoryArgs>;

export default meta;
type Story = StoryObj<IconStoryArgs>;

export const Default: Story = {
  render: ({ icon, ...args }) => {
    const C = ICONS[icon] ?? Coin;
    return (
      <span style={{ color: "var(--text)" }}>
        <C {...args} />
      </span>
    );
  },
};

export const Gallery: Story = {
  render: () => (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(8, 64px)",
        gap: 12,
        color: "var(--text)",
      }}
    >
      {Object.entries(ICONS).map(([name, C]) => (
        <div key={name} style={{ display: "grid", justifyItems: "center", gap: 6, fontSize: 9 }}>
          <C size={28} />
          <span style={{ opacity: 0.6 }}>{name}</span>
        </div>
      ))}
    </div>
  ),
};
