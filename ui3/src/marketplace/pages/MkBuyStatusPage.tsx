import type { CSSProperties } from "react";
import { MarketplaceChromeMaybe } from "../frames/MarketplaceChrome";
import ManaMark from "../../atoms/ManaMark";
import "./mkbuystatuspage.css";

type StyleVars = CSSProperties & Record<`--${string}`, string | number>;
const styleVars = (v: StyleVars): CSSProperties => v;

type BuyStatus = "pending" | "complete" | "failed" | "cancelled" | "refunded";

type StatusCopy = {
  title: string;
  status?: string;
  description: string;
  cta?: string;
};

const STATUS_COPY: Record<BuyStatus, StatusCopy> = {
  pending: {
    title: "Your item is on its way!",
    status: "Processing Transaction",
    description: "The transaction can take up to 3 minutes to be processed.",
    cta: "Back to marketplace",
  },
  complete: {
    title: "All done!",
    description: "You can find your item in {my_assets}.",
    cta: "Explore more collectibles",
  },
  failed: {
    title: "Something went wrong",
    description: "Your purchase could not be completed. If you were charged, your funds will be returned to {my_assets}.",
  },
  cancelled: {
    title: "Purchase cancelled",
    description: "This purchase was cancelled. You can try again from {my_assets}.",
  },
  refunded: {
    title: "Purchase refunded",
    description: "Your payment has been refunded. You can review the details in {my_assets}.",
  },
};

const Loader = () => <span className="mkbuystatuspage__loader" aria-hidden="true" />;

const StarsTop = () => (
  <svg className="mkbuystatuspage__stars mkbuystatuspage__stars--top" viewBox="0 0 60 60" aria-hidden="true">
    <path d="M14 2l2.2 6.6L23 11l-6.8 2.4L14 20l-2.2-6.6L5 11l6.8-2.4z" />
    <path d="M44 16l1.4 4.2L50 22l-4.6 1.8L44 28l-1.4-4.2L38 22l4.6-1.8z" opacity=".8" />
    <circle cx="32" cy="6" r="2" />
  </svg>
);
const StarsBottom = () => (
  <svg className="mkbuystatuspage__stars mkbuystatuspage__stars--bottom" viewBox="0 0 60 60" aria-hidden="true">
    <path d="M46 40l2.2 6.6L55 49l-6.8 2.4L46 58l-2.2-6.6L37 49l6.8-2.4z" />
    <path d="M16 44l1.4 4.2L22 50l-4.6 1.8L16 56l-1.4-4.2L10 50l4.6-1.8z" opacity=".8" />
    <circle cx="28" cy="54" r="2" />
  </svg>
);

type BuyAsset = {
  name: string;
  rarity: string;
  category?: string;
};

const EMPTY_ASSET: BuyAsset = {
  name: "",
  rarity: "",
};

type MkBuyStatusPageProps = {
  status?: BuyStatus;
  asset?: BuyAsset;
  chrome?: boolean;
};

export default function MkBuyStatusPage({
  status = "pending",
  asset = EMPTY_ASSET,
  chrome = true,
}: MkBuyStatusPageProps) {
  const copy = STATUS_COPY[status] || STATUS_COPY.pending;
  const isPending = status === "pending";
  const isComplete = status === "complete";

  const [descBefore, descAfter] = (copy.description || "").split("{my_assets}");

  return (
    <MarketplaceChromeMaybe chrome={chrome} active="my-assets">
      <div className="mkbuystatuspage">
        <div className="mkbuystatuspage__row">
          <div className="mkbuystatuspage__center">
            <div className="mkbuystatuspage__imgwrap">
              <StarsTop />
              <div
                className="mkbuystatuspage__img u-rar-bg"
                style={styleVars({ "--rb": `var(--rar-bg-${asset.rarity})` })}
                role="img"
                aria-label={asset.name}
              >
                <ManaMark size={64} />
              </div>
              <StarsBottom />
            </div>

            <h1 className="mkbuystatuspage__title">{copy.title}</h1>

            {isPending ? (
              <div className="mkbuystatuspage__statuswrap">
                <Loader />
                <p className="mkbuystatuspage__status">{copy.status}</p>
              </div>
            ) : null}

            <p className="mkbuystatuspage__description">
              {descBefore}
              {descAfter !== undefined ? (
                <>
                  <a className="mkbuystatuspage__link" href="/marketplace/account">
                    My Assets
                  </a>
                  {descAfter}
                </>
              ) : null}
            </p>

            {isComplete ? (
              <button type="button" className="mkbuystatuspage__cta">
                {copy.cta}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
