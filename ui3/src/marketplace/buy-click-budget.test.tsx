import { useState } from "react";
import { test, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AssetCard from "./components/AssetCard";
import MkAssetPage from "./pages/MkAssetPage";
import MkBuyFlow from "./workflows/MkBuyFlow";

const BUY_CLICK_BUDGET = 3;

function BuyJourney({ onPurchased }: { onPurchased: () => void }) {
  const [step, setStep] = useState<"card" | "asset" | "buy">("card");
  if (step === "card") {
    return (
      <AssetCard
        name="Cyber Ronin Jacket"
        rarity="legendary"
        price="1,250"
        network="polygon"
        onClick={() => setStep("asset")}
      />
    );
  }
  if (step === "asset")
    return (
      <MkAssetPage
        nft={{
          name: "Cyber Ronin Jacket",
          rarity: "legendary",
          owner: { address: "0x1" },
          collection: { name: "Cyber Ronin", address: "0x2" },
          order: { price: "1250", issuedId: 1, expiresLabel: "in 30 days" },
        }}
        onBuy={() => setStep("buy")}
      />
    );
  return <MkBuyFlow onPrimary={onPurchased} />;
}

test(`buy journey is <= ${BUY_CLICK_BUDGET} clicks (catalog -> item -> confirm)`, async () => {
  let purchased = false;
  render(<BuyJourney onPurchased={() => { purchased = true; }} />);

  let clicks = 0;
  const click = async (name: RegExp | string) => {
    clicks++;
    await userEvent.click(screen.getByRole("button", { name }));
  };

  await click(/Cyber Ronin Jacket/);
  await click("Buy");
  await click("Buy now");

  expect(purchased).toBe(true);
  expect(clicks).toBeLessThanOrEqual(BUY_CLICK_BUDGET);
});
