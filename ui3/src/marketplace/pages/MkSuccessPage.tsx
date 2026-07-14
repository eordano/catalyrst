import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import "./mksuccesspage.css";

type Asset = {
  category: string;
  name: string;
  rarity?: string;
};

type Dest = {
  getMoreNames: string;
  assignName: string;
  manageNames: string;
  manageLand: string;
  startBuilding: string;
  viewItem: string;
  tryInWorld: string;
  activity: string;
};

type SuccessState = "success" | "loading" | "error";

const COPY = {
  loading_item_title: "Your item is on its way!",
  loading_subdomain_title: "Your NAME is on its way!",
  loading_status: "Processing transaction",
  success_title: "All Done!",
  success_status: "Transaction Confirmed",
  view_item: "View my item",
  try_it_on_in_world: "Try it on In-World",
  mint_more_names: "Get more names",
  set_as_primary_name: "Assign name to Avatar",
  manage_names: "MANAGE MY NAMEs",
  manage_land: "Manage LAND",
  start_building: "Start building",
  error_title: "Oops",
  error_description: "We couldn't load the item information",
  go_to_activity: "Go to activity page",
};

const RARITY_TOKEN: Record<string, string> = {
  common: "--rar-common",
  uncommon: "--rar-uncommon",
  rare: "--rar-rare",
  epic: "--rar-epic",
  legendary: "--rar-legendary",
  mythic: "--rar-mythic",
  unique: "--rar-unique",
  exotic: "--rar-exotic",
};

function InfoCircle() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" className="mksuccesspage__infoicon">
      <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" strokeWidth="2" />
      <circle cx="12" cy="8" r="1.4" fill="currentColor" />
      <rect x="11" y="11" width="2" height="6" rx="1" fill="currentColor" />
    </svg>
  );
}

function CheckCircle() {
  return (
    <svg viewBox="0 0 28 28" width="28" height="28" aria-hidden="true" className="mksuccesspage__checkicon">
      <circle cx="14" cy="14" r="13" fill="currentColor" />
      <path d="M8.5 14.5l3.5 3.5 7.5-8" fill="none" stroke="#fff" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

type AssetImageProps = {
  category: string;
  name?: string;
  dimmed?: boolean;
};

function AssetImage({ category, name, dimmed }: AssetImageProps) {
  const token = RARITY_TOKEN[category] || "--rar-rare";
  const style: CSSProperties & { "--rar": string } = { "--rar": "var(" + token + ")" };
  return (
    <div
      className={"mksuccesspage__assetImage" + (dimmed ? " is-loading" : "")}
      style={style}
    >
      <div className="mksuccesspage__rarityBg">
        <svg viewBox="0 0 120 120" width="120" height="120" aria-hidden="true" className="mksuccesspage__assetGlyph">
          <path
            d="M60 14 L94 38 L82 96 L38 96 L26 38 Z"
            fill="rgba(255,255,255,.92)"
            stroke="rgba(0,0,0,.18)"
            strokeWidth="2"
          />
          <circle cx="60" cy="52" r="13" fill="rgba(0,0,0,.16)" />
        </svg>
        {name ? <span className="mksuccesspage__assetName">{name}</span> : null}
      </div>
    </div>
  );
}

type ActionLinkProps = {
  href: string;
  external?: boolean;
  children: ReactNode;
};

function ActionLink({ href, external, children }: ActionLinkProps) {
  const ext = external ?? /^https?:/i.test(href);
  return (
    <a
      href={href}
      style={{ display: "contents" }}
      {...(ext ? { target: "_blank", rel: "noopener noreferrer" } : null)}
    >
      {children}
    </a>
  );
}

type SuccessActionsProps = {
  asset: Asset;
  dest: Dest;
};

function SuccessActions({ asset, dest }: SuccessActionsProps) {
  if (asset.category === "ens") {
    return (
      <div className="mksuccesspage__ensActions">
        <div className="mksuccesspage__primaryEnsActions">
          <ActionLink href={dest.getMoreNames}>
            <Button variant="secondary" className="mksuccesspage__successButton">
              {COPY.mint_more_names}
            </Button>
          </ActionLink>
          <ActionLink href={dest.assignName}>
            <Button variant="primary" className="mksuccesspage__successButton">
              {COPY.set_as_primary_name}
            </Button>
          </ActionLink>
        </div>
        <ActionLink href={dest.manageNames}>
          <Button variant="ghost" className="mksuccesspage__manageBtn">
            <span className="mksuccesspage__manageNames">
              <span className="mksuccesspage__manageNamesIcon" aria-hidden="true" />
              {COPY.manage_names}
            </span>
          </Button>
        </ActionLink>
      </div>
    );
  }

  if (asset.category === "parcel" || asset.category === "estate") {
    return (
      <div className="mksuccesspage__ensActions">
        <div className="mksuccesspage__primaryEnsActions">
          <ActionLink href={dest.manageLand}>
            <Button variant="secondary" className="mksuccesspage__successButton">
              {COPY.manage_land}
            </Button>
          </ActionLink>
          <ActionLink href={dest.startBuilding}>
            <Button variant="primary" className="mksuccesspage__successButton">
              {COPY.start_building}
            </Button>
          </ActionLink>
        </div>
      </div>
    );
  }

  if (asset.category === "wearable" || asset.category === "emote") {
    return (
      <>
        <ActionLink href={dest.viewItem}>
          <Button variant="secondary" className="mksuccesspage__successButton">
            {COPY.view_item}
          </Button>
        </ActionLink>
        <div className="mksuccesspage__jumpInButtonContainer">
          <ActionLink href={dest.tryInWorld} external>
            <Button variant="primary" className="mksuccesspage__successButton">
              {COPY.try_it_on_in_world}
            </Button>
          </ActionLink>
        </div>
      </>
    );
  }

  return (
    <ActionLink href={dest.viewItem}>
      <Button variant="secondary" className="mksuccesspage__successButton">
        {COPY.view_item}
      </Button>
    </ActionLink>
  );
}

function LoadingDescription() {
  return (
    <div className="mksuccesspage__viewProgress">
      <InfoCircle />
      <div>
        The transaction can take up to <span className="mksuccesspage__highlighted">3 minutes</span>{" "}
        to be processed.
        <br />
        You can also track its progress in{" "}
        <a className="mksuccesspage__link" href="/marketplace/activity">
          Activity
        </a>
        .
      </div>
    </div>
  );
}

const EMPTY_ASSET: Asset = {
  category: "",
  name: "",
};

type MkSuccessPageProps = {
  state?: SuccessState;
  asset?: Asset;
  subdomain?: string | null;
  links?: Partial<Dest>;
  chrome?: boolean;
};

export default function MkSuccessPage({
  state = "success",
  asset = EMPTY_ASSET,
  subdomain = asset.category === "ens" ? asset.name : null,
  links = {},
  chrome = true,
}: MkSuccessPageProps) {
  const [active, setActive] = useState<MarketplaceNavId | null>(null);

  const ensName =
    asset.category === "ens" && asset.name ? String(asset.name).toLowerCase() : "";
  const dest: Dest = {
    getMoreNames: "/marketplace/claim-name",
    assignName: ensName
      ? `https://builder.decentraland.org/names/${encodeURIComponent(ensName)}`
      : "https://builder.decentraland.org/names",
    manageNames: "https://builder.decentraland.org/names",
    manageLand: "/marketplace/account",
    startBuilding: "/creator-hub",
    viewItem: "/marketplace/account",
    tryInWorld: "https://decentraland.org/play",
    activity: "/marketplace/activity",
    ...links,
  };

  let body: ReactNode;
  if (state === "error") {
    body = (
      <div className="mksuccesspage__errorContainer">
        <div className="mksuccesspage__errorInfo">
          <h1 className="mksuccesspage__errorTitle">{COPY.error_title}</h1>
          <p className="mksuccesspage__errorDescription">{COPY.error_description}</p>
        </div>
        <ActionLink href={dest.activity}>
          <Button variant="primary" className="mksuccesspage__goActivity">
            {COPY.go_to_activity}
          </Button>
        </ActionLink>
      </div>
    );
  } else if (state === "loading") {
    const title =
      subdomain || asset.category === "ens"
        ? COPY.loading_subdomain_title
        : COPY.loading_item_title;
    body = (
      <>
        <h1 className="mksuccesspage__title">{title}</h1>
        <AssetImage category={asset.category} name={asset.name} dimmed />
        <div className="mksuccesspage__statusInfo">
          <Spinner size={18} />
          {COPY.loading_status}
        </div>
        <LoadingDescription />
      </>
    );
  } else {
    body = (
      <>
        <div className="mksuccesspage__animation" aria-hidden="true" />
        <h1 className="mksuccesspage__title">{COPY.success_title}</h1>
        <AssetImage category={asset.category} name={asset.name} />
        <span className="mksuccesspage__statusInfo is-success">
          <CheckCircle />
          {COPY.success_status}
        </span>
        <div className="mksuccesspage__actionContainer">
          <SuccessActions asset={asset} dest={dest} />
        </div>
      </>
    );
  }

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={active} onTab={setActive}>
      <div className="mksuccesspage">
        <div className="mksuccesspage__container">{body}</div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
