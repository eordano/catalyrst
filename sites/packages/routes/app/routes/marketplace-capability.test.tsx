import { renderToString } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";
import BuyWizard from "@features/stories/marketplace/buy-nft/BuyWizard";
import ClaimNameWizard from "@features/stories/marketplace/claim-name/ClaimNameWizard";
import { unavailablePurchase, unavailableNameClaim } from "@data/lib/catalyst/marketplace/unavailable-actions";

vi.mock("@ui/marketplace/workflows/MkBuyFlow", () => ({ default: () => <div>Review listing</div> }));
vi.mock("@ui/marketplace/pages/MkSuccessPage", () => ({ default: () => <div>Purchased successfully</div> }));
vi.mock("@ui/marketplace/workflows/MkClaimNameWizardView", () => ({ default: ({ step }: { step: string }) => <div>Claim step: {step}</div> }));
vi.mock("@data/lib/auth/index", () => ({ useAuth: () => ({ isConnected: false }) }));

const ctx = { sid: "audit", story: "marketplace/buy-nft" as const, variant: "wizard", experimentKey: "mk_buy_wizard" };

it("does not turn a production purchase URL into a successful receipt", () => {
  const html = renderToString(<MemoryRouter initialEntries={["/marketplace/buy?step=success"]}>
    <BuyWizard allowStepPreview={false} trackCtx={ctx} track={() => {}}
      listing={{ assetId: "item", contractAddress: "0x123", tokenId: "1", priceMana: "1", priceWei: "1", network: "polygon", marketplaceAddress: null, chainId: 137, seller: "0x456" }}
      display={{ name: "Item", rarity: "rare", category: "hat", kind: "wearable" }}
      connect={unavailablePurchase} approve={unavailablePurchase} commit={unavailablePurchase} />
  </MemoryRouter>);
  expect(html).toContain("Review listing");
  expect(html).not.toContain("Purchased successfully");
});

it("does not turn a production claim URL into a minted NAME", () => {
  const html = renderToString(<MemoryRouter initialEntries={["/marketplace/claim-name?name=example&step=success"]}>
    <ClaimNameWizard allowStepPreview={false} trackCtx={{ ...ctx, story: "marketplace/claim-name" }} mint={unavailableNameClaim} track={() => {}} />
  </MemoryRouter>);
  expect(html).toContain("Claim step:");
  expect(html).not.toContain("success");
});
