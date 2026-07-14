import { siteUrl } from "../../data/site";
import type { Meta, StoryObj } from "@storybook/react-vite";

import Button from "../../atoms/Button";
import MkPaymentSection, {
  MkPaymentCardPane,
  MkPaymentManaPane,
} from "../components/MkPaymentSection";
import {
  MkCheckoutFrame,
  MkCheckoutCard,
  MkCheckoutActions,
  MkCheckoutSummary,
} from "./MkCheckout";

const LINES = [
  { key: "a", name: "ENO T-Shirt", qty: 1, unitPriceCredits: "1" },
  { key: "b", name: "Golfcraft - Eating Paella", qty: 1, unitPriceCredits: "1" },
];

const meta = {
  title: "Marketplace/Pages/Checkout",
  component: MkCheckoutFrame,
  parameters: { layout: "fullscreen" },
  args: { children: null },
} satisfies Meta<typeof MkCheckoutFrame>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ReviewSufficient: Story = {
  render: () => (
    <MkCheckoutFrame back={{ href: "#", label: "Back to cart" }} wide>
      <MkCheckoutCard>
        <div className="mkco__split">
          <div>
            <p className="mkco__lead">Spend 2 Credits on 2 items?</p>
            <p className="mkco__muted">Your balance: 24 Credits</p>
            <MkCheckoutSummary heading="You're buying" lines={LINES} total="2" />
          </div>
          <div>
            <MkCheckoutActions>
              <Button variant="primary">Confirm spend</Button>
            </MkCheckoutActions>
          </div>
        </div>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  ),
};

export const ReviewShortCardPane: Story = {
  render: () => (
    <MkCheckoutFrame back={{ href: "#", label: "Back to cart" }} wide>
      <MkCheckoutCard>
        <div className="mkco__split">
          <div>
            <p className="mkco__lead">Spend 2 Credits on 2 items?</p>
            <p className="mkco__muted">Your balance: 0 Credits</p>
            <MkCheckoutSummary heading="You're buying" lines={LINES} total="2" />
          </div>
          <div>
            <MkPaymentSection method="card" shortfallCredits="2">
              <MkPaymentCardPane />
            </MkPaymentSection>
          </div>
        </div>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  ),
};

export const ReviewShortManaPane: Story = {
  render: () => (
    <MkCheckoutFrame back={{ href: "#", label: "Back to cart" }} wide>
      <MkCheckoutCard>
        <div className="mkco__split">
          <div>
            <p className="mkco__lead">Spend 2 Credits on 2 items?</p>
            <p className="mkco__muted">Your balance: 0 Credits</p>
            <MkCheckoutSummary heading="You're buying" lines={LINES} total="2" />
          </div>
          <div>
            <MkPaymentSection method="mana" shortfallCredits="2">
              <MkPaymentManaPane
                credits="2"
                phase={{ step: "ready", quote: { weiSuggested: "775000000000000000" } }}
              />
            </MkPaymentSection>
          </div>
        </div>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  ),
};

export const Done: Story = {
  render: () => (
    <MkCheckoutFrame back={{ href: "#", label: "Back to cart" }}>
      <MkCheckoutCard tone="success">
        <p className="mkco__lead mkco__lead--success">Purchase complete!</p>
        <p className="mkco__text">Your items are on the way to your account.</p>
        <MkCheckoutSummary heading="You bought" lines={LINES} />
        <MkCheckoutActions>
          <a href={siteUrl("/play/")} className="btn btn--primary btn--md">
            Jump in world →
          </a>
          <a href="#" className="btn btn--secondary btn--md">
            View your items
          </a>
        </MkCheckoutActions>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  ),
};

export const PartialFailure: Story = {
  render: () => (
    <MkCheckoutFrame back={{ href: "#", label: "Back to cart" }}>
      <MkCheckoutCard tone="error">
        <p role="alert" className="mkco__lead mkco__lead--error">
          Order partly completed
        </p>
        <p className="mkco__text">
          Some items in this order couldn&apos;t be delivered. Everything that
          was delivered is yours; the Credits for the undelivered items were
          automatically refunded to your balance.
        </p>
        <MkCheckoutActions>
          <a href="#" className="btn btn--primary btn--md">
            View your items
          </a>
          <a href="#" className="btn btn--secondary btn--md">
            Back to marketplace
          </a>
        </MkCheckoutActions>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  ),
};
