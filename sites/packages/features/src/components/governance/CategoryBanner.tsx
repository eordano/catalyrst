import { Link } from "react-router";

import { SUBMIT_CATEGORY_ICONS } from "@ui/governance/components/SubmitCategoryIcons";

export type CategoryBannerProps = {
  type: string;
  title: string;
  description: string;
  active?: boolean;
  isNew?: boolean;
  paused?: string | null;
  notAvailable?: boolean;
  to?: string;
  onActivate?: () => void;
  onSelect?: () => void;
};

export default function CategoryBanner({
  type,
  title,
  description,
  active = true,
  isNew,
  paused,
  notAvailable,
  to,
  onActivate,
  onSelect,
}: CategoryBannerProps) {
  const Icon = SUBMIT_CATEGORY_ICONS[type];

  const className = [
    "gsp__banner",
    "gsp__banner--" + type,
    active && "is-active",
    !to && active && onActivate && "gsp__banner--clickable",
  ]
    .filter(Boolean)
    .join(" ");

  const inner = (
    <>
      <div className={"gsp__icon" + (!active ? " gsp__icon--inactive" : "")}>
        {Icon ? <Icon /> : null}
      </div>
      <div>
        <div className="gsp__titlerow">
          <h3 className="gsp__title">{title}</h3>
          {isNew && <span className="gsp__badge gsp__badge--new">New</span>}
          {!active && (
            <span className="gsp__badge gsp__badge--paused">
              {notAvailable ? "Not Available" : "Paused"}
            </span>
          )}
        </div>
        <p className="gsp__desc">{description}</p>
        {!active && paused ? <p className="gsp__pausedtext">{paused}</p> : null}
      </div>
    </>
  );

  if (!active) {
    return (
      <div className={className} aria-disabled="true">
        {inner}
      </div>
    );
  }

  if (to) {
    return (
      <Link className={className} to={to} onClick={() => onSelect?.()}>
        {inner}
      </Link>
    );
  }

  return (
    <button
      type="button"
      className={className}
      onClick={() => {
        onSelect?.();
        onActivate?.();
      }}
    >
      {inner}
    </button>
  );
}
