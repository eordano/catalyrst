import type { ReactNode } from "react";
import { useId } from "react";
import CreatorHubChrome, { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import Modal from "../../components/Modal";
import Button from "../../atoms/Button";
import { Close } from "../../atoms/icons";
import "./chmodalworldsyourstorage.css";

const COPY = {
  yourStorage: "Your Storage",
  totalAvailableStorage: "TOTAL AVAILABLE STORAGE",
  mana: "MANA",
  manaEarnStorage:
    "Earn 100 Mb of storage per 2,000 tokens (Polygon or Ethereum).",
  manaHoldings: (mbs: number, owned: number) =>
    `You have ${mbs} Mb thanks to holding ${owned} MANA tokens.`,
  manaBuy: "BUY MANA",
  lands: "LANDs",
  landsEarnStorage: "Earn 100 Mb of storage per LAND.",
  landsHoldings: (mbs: number, owned: number) =>
    `You have ${mbs} Mb thanks to holding ${owned} LANDs.`,
  landsBuy: "BUY LAND",
  names: "NAMEs",
  namesEarnStorage: "Earn 100 Mb of storage per NAME.",
  namesHoldings: (mbs: number, owned: number) =>
    `You have ${mbs} Mb thanks to holding ${owned} Decentraland NAMEs.`,
  namesBuy: "BUY NAME",
  proposalPrefix: "These storage rules were voted on and passed in a ",
  proposalLinkText: "governance DAO proposal",
  learnMore: "LEARN MORE",
};

const PROPOSAL_URL =
  "https://governance.decentraland.org/proposal/?id=c3216070-e822-11ed-b8f1-75dbe089d333";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

function formatSize(size: number) {
  if (size < KB) return `${size.toFixed(2)} B`;
  if (size < MB) return `${(size / KB).toFixed(2)} KB`;
  if (size < GB) return `${(size / MB).toFixed(2)} MB`;
  return `${(size / GB).toFixed(2)} GB`;
}

type AccountHoldings = {
  ownedMana: number;
  ownedLands: number;
  ownedNames: number;
  ownedLandMbs?: number;
};

function getMbsFromAccountHoldings(h: AccountHoldings) {
  return {
    manaMbs: Math.trunc(h.ownedMana / 2000) * 100,
    landMbs: h.ownedLands * 100,
    nameMbs: h.ownedNames * 100,
  };
}

function OpenInNewIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <path
        fill="currentColor"
        d="M19 19H5V5h7V3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7h-2v7ZM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7Z"
      />
    </svg>
  );
}

function CheckCircleIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="chmodalworldsyourstorage__check-icon"
      aria-hidden="true"
    >
      <path
        fill="currentColor"
        d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm-2 15-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9Z"
      />
    </svg>
  );
}

function InfoIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="chmodalworldsyourstorage__info-icon"
      aria-hidden="true"
    >
      <path
        fill="currentColor"
        d="M11 7h2v2h-2V7Zm0 4h2v6h-2v-6Zm1-9a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16Z"
      />
    </svg>
  );
}

function isExternalHref(href: string) {
  return /^(https?:)?\/\//i.test(href);
}

type LinkButtonProps = {
  label: ReactNode;
  href?: string;
  onClick?: () => void;
};

const BUY_BTN_CLASS = "btn btn--primary btn--md chmodalworldsyourstorage__btn";

function LinkButton({ label, href, onClick }: LinkButtonProps) {
  if (href) {
    return (
      <a
        className={BUY_BTN_CLASS}
        href={href}
        {...(isExternalHref(href)
          ? { target: "_blank", rel: "noreferrer" }
          : {})}
      >
        {label}
        <OpenInNewIcon />
      </a>
    );
  }
  if (onClick) {
    return (
      <Button
        variant="primary"
        className="chmodalworldsyourstorage__btn"
        onClick={onClick}
      >
        {label}
        <OpenInNewIcon />
      </Button>
    );
  }
  return null;
}

type AssetRowProps = {
  name: ReactNode;
  subtitle: ReactNode;
  holdingsLine: ReactNode;
  buyLabel: ReactNode;
  buyHref?: string;
  onBuy?: () => void;
};

function AssetRow({ name, subtitle, holdingsLine, buyLabel, buyHref, onBuy }: AssetRowProps) {
  return (
    <div className="chmodalworldsyourstorage__asset">
      <div className="chmodalworldsyourstorage__texts">
        <span className="chmodalworldsyourstorage__name">{name}</span>
        <span className="chmodalworldsyourstorage__subtitle">{subtitle}</span>
        {holdingsLine ? (
          <span className="chmodalworldsyourstorage__amount">
            <CheckCircleIcon />
            {holdingsLine}
          </span>
        ) : null}
      </div>
      <LinkButton label={buyLabel} href={buyHref} onClick={onBuy} />
    </div>
  );
}

type ChModalWorldsYourStorageProps = {
  variant?: "modal" | "panel";
  open?: boolean;
  embedded?: boolean;
  chrome?: boolean;
  stats?: { usedSpace: string; maxAllowedSpace: string };
  accountHoldings?: AccountHoldings | null;
  onClose?: () => void;
  manaHref?: string;
  landHref?: string;
  nameHref?: string;
  learnMoreHref?: string;
  onBuyMana?: () => void;
  onBuyLand?: () => void;
  onBuyName?: () => void;
  onLearnMore?: () => void;
};

export default function ChModalWorldsYourStorage({
  variant = "modal",
  open = true,
  embedded = false,
  chrome = true,
  stats = { usedSpace: "0", maxAllowedSpace: "0" },
  accountHoldings = undefined,
  onClose = undefined,
  manaHref = "",
  landHref = "",
  nameHref = "",
  learnMoreHref = "",
  onBuyMana = undefined,
  onBuyLand = undefined,
  onBuyName = undefined,
  onLearnMore = undefined,
}: ChModalWorldsYourStorageProps) {
  const titleId = useId();
  const framed = chrome && !embedded;

  if (!open) {
    return framed ? <CreatorHubChrome active="manage" /> : null;
  }

  const totalAvailable = formatSize(
    Number(stats.maxAllowedSpace) - Number(stats.usedSpace),
  );
  const mbs = accountHoldings ? getMbsFromAccountHoldings(accountHoldings) : null;

  const card = (
    <>
      <h5 className="chmodalworldsyourstorage__title" id={titleId}>
        {COPY.yourStorage}
      </h5>

      <div className="chmodalworldsyourstorage__content">
        <div className="chmodalworldsyourstorage__total-storage">
          <span>{COPY.totalAvailableStorage}</span>
          <span className="chmodalworldsyourstorage__mbs">
            {totalAvailable}
          </span>
        </div>

        <AssetRow
          name={COPY.mana}
          subtitle={COPY.manaEarnStorage}
          holdingsLine={
            accountHoldings && mbs && mbs.manaMbs > 0
              ? COPY.manaHoldings(
                  mbs.manaMbs,
                  Math.trunc(accountHoldings.ownedMana),
                )
              : null
          }
          buyLabel={COPY.manaBuy}
          buyHref={manaHref}
          onBuy={onBuyMana}
        />
        <hr className="chmodalworldsyourstorage__separator" />
        <AssetRow
          name={COPY.lands}
          subtitle={COPY.landsEarnStorage}
          holdingsLine={
            accountHoldings && mbs && mbs.landMbs > 0
              ? COPY.landsHoldings(mbs.landMbs, accountHoldings.ownedLands)
              : null
          }
          buyLabel={COPY.landsBuy}
          buyHref={landHref}
          onBuy={onBuyLand}
        />
        <hr className="chmodalworldsyourstorage__separator" />
        <AssetRow
          name={COPY.names}
          subtitle={COPY.namesEarnStorage}
          holdingsLine={
            accountHoldings && mbs && mbs.nameMbs > 0
              ? COPY.namesHoldings(mbs.nameMbs, accountHoldings.ownedNames)
              : null
          }
          buyLabel={COPY.namesBuy}
          buyHref={nameHref}
          onBuy={onBuyName}
        />

        <p className="chmodalworldsyourstorage__proposal">
          <InfoIcon />
          <span>
            {COPY.proposalPrefix}
            <a
              className="chmodalworldsyourstorage__learn-more"
              href={PROPOSAL_URL}
              target="_blank"
              rel="noreferrer"
            >
              {COPY.proposalLinkText}
            </a>
            .
          </span>{" "}
          {learnMoreHref ? (
            <a
              className="chmodalworldsyourstorage__learn-more"
              href={learnMoreHref}
              {...(isExternalHref(learnMoreHref)
                ? { target: "_blank", rel: "noreferrer" }
                : {})}
            >
              {COPY.learnMore}
            </a>
          ) : onLearnMore ? (
            <button
              type="button"
              className="chmodalworldsyourstorage__learn-more"
              onClick={onLearnMore}
            >
              {COPY.learnMore}
            </button>
          ) : null}
        </p>
      </div>
    </>
  );

  const surface =
    variant === "panel" ? (
      <section
        className="chmodalworldsyourstorage chmodalworldsyourstorage--panel"
        role="region"
        aria-labelledby={titleId}
      >
        {onClose ? (
          <button
            type="button"
            className="chmodalworldsyourstorage__close"
            aria-label="Close"
            onClick={onClose}
          >
            <Close />
          </button>
        ) : null}
        {card}
      </section>
    ) : (
      <Modal
        width={720}
        className="chmodalworldsyourstorage"
        ariaLabelledBy={titleId}
        onClose={onClose}
      >
        {card}
      </Modal>
    );

  return (
    <CreatorHubChromeMaybe chrome={framed} active="manage">
      {surface}
    </CreatorHubChromeMaybe>
  );
}
