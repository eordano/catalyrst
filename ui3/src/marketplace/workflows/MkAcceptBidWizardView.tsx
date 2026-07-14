import MkMyBids from "../pages/MkMyBids";
import MkSuccessPage from "../pages/MkSuccessPage";
import AssetActionLayout from "../frames/AssetActionLayout";
import Spinner from "../../atoms/Spinner";
import Button from "../../atoms/Button";
import "./mkacceptbidwizardview.css";

export type MkAcceptBidWizardBid = {
  id: string;
  bidder: string;
  bidderName: string;
  priceMana: string;
  network: string;
  createdRelative: string;
  timeLeft: string;
  asset: { name: string; category: string; rarity: string };
};

export type MkAcceptBidWizardViewProps = {
  bid: MkAcceptBidWizardBid;
  value: string;
  step: string;
  error?: string;
  txHash?: string;
  onReject: () => void;
  onAccept: () => void;
  onBack: () => void;
  onConnect: () => void;
  onConfirm: () => void;
  onRetry: () => void;
};

export default function MkAcceptBidWizardView({
  bid,
  value,
  step,
  error,
  txHash,
  onReject,
  onAccept,
  onBack,
  onConnect,
  onConfirm,
  onRetry,
}: MkAcceptBidWizardViewProps) {
  const media = <AssetPreviewTile bid={bid} />;
  const priceLine = (
    <span className="acceptbid__price">
      <span className="acceptbid__mana" aria-hidden="true">&#x25C7;</span>
      {bid.priceMana} MANA
    </span>
  );

  return (
    <div className="acceptbid-wizard" data-step={step}>
      {value === "reviewBid" && (
        <div className="acceptbid__review">
          <MkMyBids
            sellerBids={[
              {
                id: bid.id,
                name: bid.asset.name,
                bidder: bid.bidderName || shortAddr(bid.bidder),
                hue: 268,
                tile: `var(--rar-${bid.asset.rarity})`,
                created: bid.createdRelative,
                price: bid.priceMana,
                timeLeft: bid.timeLeft,
              },
            ]}
            archivedBids={[]}
            bidderBids={[]}
          />
          <div className="acceptbid__reviewbar" role="group" aria-label="Respond to bid">
            <div className="acceptbid__reviewinfo">
              Bid on <strong>{bid.asset.name}</strong> for {priceLine}
            </div>
            <div className="acceptbid__btns">
              <Button variant="secondary" onClick={onReject}>
                Reject bid
              </Button>
              <Button variant="primary" onClick={onAccept}>
                Accept bid
              </Button>
            </div>
          </div>
        </div>
      )}

      {value === "connectWallet" && (
        <AssetActionLayout
          theme="dark"
          media={media}
          title="Sign in"
          subtitle={<>To accept this bid you need to connect the wallet that owns <strong>{bid.asset.name}</strong>.</>}
          warning={null}
          icon={null}
          onBack={onBack}
        >
          <div className="acceptbid__btns">
            <Button variant="primary" onClick={onConnect}>
              Sign in
            </Button>
          </div>
        </AssetActionLayout>
      )}

      {value === "approveNft" && (
        <AssetActionLayout
          theme="dark"
          variant="status"
          hideBack
          iconTone="neutral"
          icon={<Spinner size={28} color="currentColor" />}
          media={null}
          warning={null}
          onBack={() => {}}
          title="Approving your item"
          subtitle={
            <>
              Granting the marketplace trade contract permission to transfer{" "}
              <strong>{bid.asset.name}</strong> on accept. (Simulated &#x2014; no wallet
              transaction is sent.)
            </>
          }
        >
          <div className="acceptbid__note">This usually takes a few seconds&#x2026;</div>
        </AssetActionLayout>
      )}

      {value === "confirmAccept" && (
        <AssetActionLayout
          theme="dark"
          media={media}
          title="Confirm bid acceptance"
          subtitle={<>You are about to sell <strong>{bid.asset.name}</strong>. This is irreversible.</>}
          warning={
            <>Accepting transfers the NFT to {shortAddr(bid.bidder)} in exchange for {priceLine}.</>
          }
          icon={null}
          onBack={onBack}
        >
          <dl className="acceptbid__terms">
            <div><dt>You receive</dt><dd>{priceLine}</dd></div>
            <div><dt>Buyer</dt><dd>{bid.bidderName || shortAddr(bid.bidder)}</dd></div>
            <div><dt>Network</dt><dd>{bid.network}</dd></div>
            <div><dt>Bid expires</dt><dd>{bid.timeLeft}</dd></div>
          </dl>
          <div className="acceptbid__btns">
            <Button variant="primary" onClick={onConfirm}>
              Confirm &amp; accept
            </Button>
          </div>
        </AssetActionLayout>
      )}

      {value === "submitTx" && (
        <MkSuccessPage state="loading" asset={successAsset(bid)} />
      )}

      {value === "success" && (
        <div className="acceptbid__success">
          <MkSuccessPage state="success" asset={successAsset(bid)} />
          <div className="acceptbid__txnote" role="status">
            Sold for {priceLine}. Simulated tx{" "}
            <code>{txHash}</code> (stub &#x2014; no on-chain write).
          </div>
        </div>
      )}

      {value === "rejected" && (
        <AssetActionLayout
          theme="dark"
          variant="status"
          hideBack
          media={null}
          warning={null}
          icon={null}
          onBack={() => {}}
          title="Bid rejected"
          subtitle={<>You declined the bid on <strong>{bid.asset.name}</strong>. It will remain listed for other offers.</>}
        >
          <div className="acceptbid__btns">
            <Button variant="secondary" onClick={onBack}>
              Back to bids
            </Button>
          </div>
        </AssetActionLayout>
      )}

      {value === "error" && (
        <AssetActionLayout
          theme="dark"
          variant="status"
          hideBack
          subtitleTone="danger"
          media={null}
          warning={null}
          icon={null}
          onBack={() => {}}
          title="Something went wrong"
          subtitle={error ?? "The accept could not be completed."}
        >
          <div className="acceptbid__btns">
            <Button variant="primary" onClick={onRetry}>
              Retry
            </Button>
          </div>
        </AssetActionLayout>
      )}
    </div>
  );
}

function AssetPreviewTile({ bid }: { bid: MkAcceptBidWizardBid }) {
  return (
    <div
      className="acceptbid__tile"
      style={{ ["--tile" as string]: `var(--rar-${bid.asset.rarity})` }}
      role="img"
      aria-label={bid.asset.name}
    >
      <span className="acceptbid__tilename">{bid.asset.name}</span>
    </div>
  );
}

function successAsset(bid: MkAcceptBidWizardBid) {
  return {
    category: bid.asset.category,
    name: bid.asset.name,
    rarity: bid.asset.rarity,
  };
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 11) return addr || "the buyer";
  return `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}`;
}
