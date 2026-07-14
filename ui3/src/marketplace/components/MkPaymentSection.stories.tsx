import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";

import MkPaymentSection, {
  MkPaymentCardPane,
  MkPaymentManaPane,
  type MkPayMethod,
} from "./MkPaymentSection";

const meta = {
  title: "Marketplace/Components/PaymentSection",
  component: MkPaymentSection,
  parameters: { layout: "padded" },
} satisfies Meta<typeof MkPaymentSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Interactive: Story = {
  render: () => {
    const [method, setMethod] = useState<MkPayMethod | null>(null);
    return (
      <MkPaymentSection method={method} shortfallCredits="3" onPickMethod={setMethod}>
        {method === "card" && <MkPaymentCardPane />}
        {method === "mana" && (
          <MkPaymentManaPane
            credits="3"
            phase={{ step: "ready", quote: { weiSuggested: "1160000000000000000" } }}
          />
        )}
      </MkPaymentSection>
    );
  },
};

export const CardPaying: Story = {
  render: () => (
    <MkPaymentSection method="card" shortfallCredits="3">
      <MkPaymentCardPane phase="paying" />
    </MkPaymentSection>
  ),
};

export const CardDone: Story = {
  render: () => (
    <MkPaymentSection method="card" shortfallCredits="3">
      <MkPaymentCardPane phase="done" granted="3" />
    </MkPaymentSection>
  ),
};

export const CardRailOff: Story = {
  render: () => (
    <MkPaymentSection method="card" shortfallCredits="3">
      <MkPaymentCardPane phase="off" />
    </MkPaymentSection>
  ),
};

export const ManaConfirming: Story = {
  render: () => (
    <MkPaymentSection method="mana" shortfallCredits="3">
      <MkPaymentManaPane
        credits="3"
        phase={{ step: "confirming", txHash: "0xabc123def4567890abc123def4567890abc123def4567890abc123def4567890" }}
      />
    </MkPaymentSection>
  ),
};

export const ManaDone: Story = {
  render: () => (
    <MkPaymentSection method="mana" shortfallCredits="3">
      <MkPaymentManaPane credits="3" phase={{ step: "done", granted: "3" }} />
    </MkPaymentSection>
  ),
};

export const ManaUnavailable: Story = {
  render: () => (
    <MkPaymentSection method="mana" shortfallCredits="3">
      <MkPaymentManaPane
        credits="3"
        phase={{ step: "unavailable", why: "MANA payments aren't available right now." }}
      />
    </MkPaymentSection>
  ),
};
