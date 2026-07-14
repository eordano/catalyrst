import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { useState } from "react";
import FilterBox from "./FilterBox";

const meta = {
  tags: ["autodocs"],
  title: "Components/FilterBox",
  component: FilterBox,
  parameters: { layout: "centered" },
} satisfies Meta<typeof FilterBox>;

export default meta;
type Story = StoryObj<typeof meta>;

const Shell = ({ children }: { children: ReactNode }) => (
  <div style={{ width: 256 }}>{children}</div>
);

function Demo({ size }: { size: "caps" | "title" }) {
  const [open, setOpen] = useState({ category: true, rarity: true, price: false });
  const toggle = (k: keyof typeof open) => setOpen((o) => ({ ...o, [k]: !o[k] }));
  return (
    <Shell>
      <FilterBox title="Category" size={size} open={open.category} onToggle={() => toggle("category")}>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {["Head", "Upper body", "Lower body", "Feet"].map((c) => (
            <span key={c} style={{ color: "var(--ink-7)", fontSize: 13, fontWeight: 600 }}>{c}</span>
          ))}
        </div>
      </FilterBox>
      <FilterBox title="Rarity" size={size} open={open.rarity} onToggle={() => toggle("rarity")}>
        <span style={{ color: "var(--ink-7)", fontSize: 13 }}>Common &#xB7; Rare &#xB7; Epic &#xB7; Legendary</span>
      </FilterBox>
      <FilterBox title="Price" size={size} open={open.price} onToggle={() => toggle("price")}>
        <span style={{ color: "var(--ink-7)", fontSize: 13 }}>Min &#x2014; Max</span>
      </FilterBox>
    </Shell>
  );
}

export const Caps: Story = { render: () => <Demo size="caps" /> };

export const Title: Story = { render: () => <Demo size="title" /> };
