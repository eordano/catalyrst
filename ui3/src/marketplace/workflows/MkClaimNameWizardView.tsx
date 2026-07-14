import type { ReactNode } from "react";

import MkClaimNamePage from "../pages/MkClaimNamePage";
import MkSuccessPage from "../pages/MkSuccessPage";
import AssetActionLayout from "../frames/AssetActionLayout";
import Button from "../../atoms/Button";
import ManaMark from "../../atoms/ManaMark";
import "../frames/assetactionlayout.css";
import "./mkclaimnamewizardview.css";

export type MkClaimNameWizardViewProps = {
  value: string;
  step: string;
  activeName: string;
  initialName: string;
  priceMana: string;
  banner?: ReactNode;
  creditsNote?: string;
  onClaim: (name: string) => void;
  onBack: () => void;
  onApproveMana: () => void;
  onConfirmMint: () => void;
  onRetry: () => void;
  onReturnToPublish?: (worldName: string) => void;
};

export default function MkClaimNameWizardView({
  value,
  step,
  activeName,
  initialName,
  priceMana,
  banner,
  creditsNote,
  onClaim,
  onBack,
  onApproveMana,
  onConfirmMint,
  onRetry,
  onReturnToPublish,
}: MkClaimNameWizardViewProps) {
  return (
    <div className="claim-name-wizard" data-step={step}>
      {(value === "entering" || value === "checking" || value === "unavailable") && (
        <>
          <MkClaimNamePage
            initialName={initialName}
            initialFocused={value !== "entering"}
            banner={banner}
            creditsNote={creditsNote}
            forceStatus={
              value === "checking"
                ? { kind: "available" }
                : value === "unavailable"
                  ? { kind: "unavailable" }
                  : undefined
            }
            onClaim={onClaim}
          />
        </>
      )}

      {value === "approving" && (
        <AssetActionLayout
          theme="dark"
          media={null}
          icon={null}
          title={`Approve MANA to claim ${activeName}.dcl.eth`}
          subtitle={
            <>
              Claiming a NAME costs <ManaMark className="" /> {priceMana} MANA on
              Ethereum Mainnet. Approve the DCLRegistrar to spend your MANA.
            </>
          }
          warning={"Test mode \u{2014} this approval is simulated: no transaction is sent and no MANA leaves your wallet."}
          onBack={onBack}
        >
          <div className="claim-name-wizard__controls">
            <Button variant="primary" onClick={onApproveMana}>
              Approve {priceMana} MANA
            </Button>
          </div>
        </AssetActionLayout>
      )}

      {value === "confirming" && (
        <AssetActionLayout
          theme="dark"
          media={null}
          icon={null}
          title={`Confirm minting ${activeName}.dcl.eth`}
          subtitle={
            <>
              You are about to mint <strong>{activeName}.dcl.eth</strong> for{" "}
              <ManaMark className="" /> {priceMana} MANA. This grants a Decentraland
              NAME, a World, and extra Voting Power.
            </>
          }
          warning={"Test mode \u{2014} this mint is simulated: no NAME will actually be registered and nothing will be charged."}
          onBack={onBack}
        >
          <div className="claim-name-wizard__controls">
            <Button variant="primary" onClick={onConfirmMint}>
              Confirm & mint
            </Button>
          </div>
        </AssetActionLayout>
      )}

      {value === "submitting" && (
        <MkSuccessPage
          state="loading"
          asset={{ category: "ens", name: activeName, rarity: "rare" }}
        />
      )}

      {value === "success" && (
        <>
          <MkSuccessPage
            state="success"
            asset={{ category: "ens", name: activeName, rarity: "rare" }}
          />
          {onReturnToPublish && (
            <div className="claim-name-wizard__controls claim-name-wizard__controls--overlay">
              <Button
                variant="primary"
                onClick={() =>
                  onReturnToPublish(`${activeName.toLowerCase()}.dcl.eth`)
                }
              >
                Use in Publish to World
              </Button>
            </div>
          )}
        </>
      )}

      {value === "error" && (
        <>
          <MkSuccessPage
            state="error"
            asset={{ category: "ens", name: activeName, rarity: "rare" }}
          />
          <div className="claim-name-wizard__controls claim-name-wizard__controls--overlay">
            <Button variant="primary" onClick={onRetry}>
              Retry mint
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
