import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import PriceRange from "./PriceRange";

const meta = {
  title: "Marketplace/Components/PriceRange",
  component: PriceRange,
  parameters: { layout: "centered" },
} satisfies Meta<typeof PriceRange>;

export default meta;
type Story = StoryObj<typeof meta>;

const Panel = ({ children }: { children?: ReactNode }) => (
  <div style={{ width: 240, background: "var(--panel)", border: "1px solid var(--line)", borderRadius: "var(--r-panel)", padding: 16 }}>
    {children}
  </div>
);

export const Default: Story = { render: () => <Panel><PriceRange /></Panel> };
