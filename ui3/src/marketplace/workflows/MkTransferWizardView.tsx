import MarketplaceChrome, { type MarketplaceNavId } from "../frames/MarketplaceChrome";
import MkTransferPage from "../pages/MkTransferPage";
import MkSuccessPage from "../pages/MkSuccessPage";
import Web3Confirm from "../../web/workflows/Web3Confirm";
import AssetActionLayout from "../frames/AssetActionLayout";
import AssetPreviewTile from "../components/AssetPreviewTile";
import Button from "../../atoms/Button";
import "./mktransferwizardview.css";

export type MkTransferWizardAsset = {
  id: string;
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
  image?: string | null;
};

export type MkTransferWizardViewProps = {
  value: string;
  step: string;
  source: "catalyst" | "fixture";
  assets: MkTransferWizardAsset[];
  activeAsset?: MkTransferWizardAsset;
  tab: MarketplaceNavId;
  onTab: (tab: MarketplaceNavId) => void;
  recipient: string;
  recipientInvalid: boolean;
  recipientShort: string;
  txHashShort: string;
  error?: string;
  onRecipientChange: (value: string) => void;
  onSelectAsset: (asset: MkTransferWizardAsset) => void;
  onSubmitRecipient: () => void;
  onBack: () => void;
  onConfirm: () => void;
  onApprove: () => void;
  onExit: () => void;
  onRetry: () => void;
};

export default function MkTransferWizardView({
  value,
  step,
  source,
  assets,
  activeAsset,
  tab,
  onTab,
  recipient,
  recipientInvalid,
  recipientShort,
  txHashShort,
  error,
  onRecipientChange,
  onSelectAsset,
  onSubmitRecipient,
  onBack,
  onConfirm,
  onApprove,
  onExit,
  onRetry,
}: MkTransferWizardViewProps) {
  const firstAsset = assets[0];

  if (value === "selecting") {
    return (
      <MarketplaceChrome active={tab} onTab={onTab}>
        <div className="transfer-wizard" data-step={step}>
          <SimulationNote source={source} />
          <AssetActionLayout
            theme="dark"
            backLabel={undefined}
            hideBack
            title="Transfer an item"
            subtitle="Select an item you own to transfer to another wallet."
            warning={null}
            icon={null}
            onBack={() => {}}
            media={
              firstAsset ? (
                <AssetPreviewTile rarity={firstAsset.rarity} figure={firstAsset.image ? "none" : "inset"} label={undefined} image={firstAsset.image} style={{}}>{null}</AssetPreviewTile>
              ) : null
            }
          >
            {assets.length === 0 ? (
              <p className="transfer-wizard__empty">
                This wallet has no transferable items.
              </p>
            ) : (
              <ul className="transfer-wizard__assets" aria-label="Owned items">
                {assets.map((a) => (
                  <li key={a.id}>
                    <button
                      type="button"
                      className="transfer-wizard__asset"
                      onClick={() => onSelectAsset(a)}
                    >
                      <span className={"transfer-wizard__rarity is-" + a.rarity} aria-hidden="true" />
                      <span className="transfer-wizard__assetname">{a.name}</span>
                      <span className="transfer-wizard__assetmeta">
                        {a.category} &#xB7; {a.network}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </AssetActionLayout>
        </div>
      </MarketplaceChrome>
    );
  }

  if (value === "enteringRecipient") {
    return (
      <div className="transfer-wizard" data-step={step}>
        <MkTransferPage
          nft={mkNft(activeAsset)}
          status="form"
        />
        <div className="transfer-wizard__controls" role="group" aria-label="Enter recipient">
          <label className="transfer-wizard__label" htmlFor="transfer-recipient">
            Recipient address
          </label>
          <input
            id="transfer-recipient"
            type="text"
            className={"transfer-wizard__input" + (recipientInvalid ? " is-error" : "")}
            placeholder="0x..."
            value={recipient}
            onChange={(e) => onRecipientChange(e.target.value)}
          />
          {recipientInvalid && (
            <div className="transfer-wizard__error">That&apos;s not a valid address</div>
          )}
          <div className="transfer-wizard__btnrow">
            <Button variant="secondary" onClick={onBack}>
              Back
            </Button>
            <Button variant="primary" onClick={onSubmitRecipient}>
              Continue
            </Button>
          </div>
        </div>
      </div>
    );
  }

  if (value === "reviewing") {
    return (
      <MarketplaceChrome active={tab} onTab={onTab}>
        <div className="transfer-wizard" data-step={step}>
          <AssetActionLayout
            theme="dark"
            backLabel={undefined}
            hideBack
            title="Review transfer"
            subtitleTone="danger"
            subtitle={
              <>
                Transferring is an <b>irreversible</b> operation. Please check the
                recipient carefully.
              </>
            }
            warning={null}
            icon={null}
            onBack={() => {}}
            media={
              activeAsset ? (
                <AssetPreviewTile rarity={activeAsset.rarity} figure={activeAsset.image ? "none" : "inset"} label={undefined} image={activeAsset.image} style={{}}>{null}</AssetPreviewTile>
              ) : null
            }
          >
            <dl className="transfer-wizard__summary">
              <dt>Item</dt>
              <dd>{activeAsset?.name}</dd>
              <dt>Type</dt>
              <dd>{activeAsset?.category}</dd>
              <dt>Recipient</dt>
              <dd className="transfer-wizard__mono">{recipientShort}</dd>
            </dl>
            <div className="transfer-wizard__btnrow">
              <Button variant="secondary" onClick={onBack}>
                Back
              </Button>
              <Button variant="primary" onClick={onConfirm}>
                Confirm transfer
              </Button>
            </div>
          </AssetActionLayout>
        </div>
      </MarketplaceChrome>
    );
  }

  if (value === "confirming") {
    return (
      <div className="transfer-wizard" data-step={step}>
        <Web3Confirm
          code="824173"
          expiry="05:00"
          onBack={onBack}
          onExit={onExit}
        />
        <div className="transfer-wizard__controls transfer-wizard__controls--floating" role="group" aria-label="Wallet confirmation">
          <p className="transfer-wizard__hint">
            Approve the transfer in your wallet (simulated &#x2014; no transaction is
            signed or broadcast).
          </p>
          <Button variant="primary" onClick={onApprove}>
            Approve in wallet
          </Button>
        </div>
      </div>
    );
  }

  if (value === "submitting") {
    return (
      <div className="transfer-wizard" data-step={step}>
        <MkTransferPage nft={mkNft(activeAsset)} status="transferring" />
      </div>
    );
  }

  if (value === "success") {
    return (
      <div className="transfer-wizard" data-step={step}>
        <MkSuccessPage
          state="success"
          asset={{
            category: activeAsset?.category ?? "wearable",
            name: activeAsset?.name ?? "",
            rarity: activeAsset?.rarity ?? "rare",
          }}
        />
        <div className="transfer-wizard__txnote" role="status">
          Transfer submitted (simulated). Tx{" "}
          <span className="transfer-wizard__mono">
            {txHashShort}
          </span>{" "}
          &#x2014; stub, not broadcast on-chain.
        </div>
      </div>
    );
  }

  return (
    <div className="transfer-wizard" data-step={step}>
      <MkSuccessPage state="error" />
      <div className="transfer-wizard__controls" role="group" aria-label="Transfer error">
        <p className="transfer-wizard__error">{error ?? "Transfer failed."}</p>
        <div className="transfer-wizard__btnrow">
          <Button variant="secondary" onClick={onBack}>
            Back to review
          </Button>
          <Button variant="primary" onClick={onRetry}>
            Retry
          </Button>
        </div>
      </div>
    </div>
  );
}

function SimulationNote({ source }: { source: "catalyst" | "fixture" }) {
  return (
    <p className="transfer-wizard__note">
      {source === "catalyst" ? "Owned items from catalyst." : "Owned items from bundled fixture (catalyst empty/unreachable)."}{" "}
      On-chain transfer is simulated &#x2014; no transaction is signed or broadcast.
    </p>
  );
}

function mkNft(asset: MkTransferWizardAsset | undefined) {
  if (!asset) return undefined;
  const parts = asset.id.split("-");
  return {
    contractAddress: parts[0] ?? "",
    tokenId: parts[parts.length - 1] ?? "",
    name: asset.name,
    category: asset.category,
    rarity: asset.rarity,
    network: asset.network,
  };
}
