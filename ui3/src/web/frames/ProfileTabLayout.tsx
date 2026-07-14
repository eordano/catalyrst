import type { CSSProperties, ReactNode } from "react";
import { Copy } from "../../atoms/icons";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./profiletablayout.css";
import { hueFor } from "../../data/format";

type Profile = {
  address: string;
  name?: string;
  nameColor?: string;
  avatarUrl?: string;
  hasClaimedName?: boolean;
};

type Tab = { id: string; label: ReactNode };

function truncateAddress(addr: string) {
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}


const WalletGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M3 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v1h1a1 1 0 0 1 1 1v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm14 7a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);
const VerifiedGlyph = () => (
  <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
    <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

type ProfileTabLayoutProps = LabelSuffixProps & {
  profile: Profile;
  tabs?: Tab[];
  activeTab?: string;
  onTabChange?: (id: string) => void;
  actions?: ReactNode;
  aside?: ReactNode;
  onCopyAddress?: () => void;
  children?: ReactNode;
  showWalletIcon?: boolean;
  showCopy?: boolean;
  bodyPadded?: boolean;
  pageBackground?: string;
  className?: string;
};

export default function ProfileTabLayout({
  labelSuffix,
  profile,
  tabs = [],
  activeTab,
  onTabChange,
  actions,
  aside,
  onCopyAddress,
  children,
  showWalletIcon = true,
  showCopy = true,
  bodyPadded = false,
  pageBackground,
  className,
}: ProfileTabLayoutProps) {
  const initial = (profile.name || profile.address).charAt(0).toUpperCase();
  const hasClaimedName = profile.hasClaimedName ?? false;
  const nameColor = profile.nameColor;
  const avatarUrl = profile.avatarUrl;

  const avatarStyle: CSSProperties = nameColor
    ? { background: nameColor }
    : avatarUrl
      ? { backgroundImage: `url(${avatarUrl})`, backgroundSize: "cover", backgroundPosition: "center" }
      : ({ "--hue": hueFor(profile.name || profile.address || "") } as CSSProperties);
  const avatarClass = "ptl__avatar" + (!nameColor && !avatarUrl ? " u-avatar" : "");

  const pageStyle: CSSProperties | undefined = pageBackground ? { background: pageBackground } : undefined;

  return (
    <div className={"ptl" + (className ? " " + className : "")} style={pageStyle}>
      <div className="ptl__content">
        <div className="ptl__card">
          <section className="ptl__header" aria-label={suffixLabel("Profile summary", labelSuffix)}>
            <div className="ptl__identity">
              <span className={avatarClass} style={avatarStyle} aria-hidden="true">
                {!avatarUrl ? <span className="ptl__avatarinitial">{initial}</span> : null}
              </span>
              <div className="ptl__nameblock">
                <div className="ptl__namerow">
                  <span className="ptl__name" style={nameColor ? { color: nameColor } : undefined}>
                    {profile.name}
                  </span>
                  {hasClaimedName ? (
                    <span
                      className="ptl__verified"
                      style={nameColor ? { background: nameColor } : undefined}
                      title="Verified"
                    >
                      <VerifiedGlyph />
                    </span>
                  ) : (
                    <span className="ptl__discriminator">#{profile.address.slice(-4)}</span>
                  )}
                </div>
                <div className="ptl__addrrow">
                  {showWalletIcon ? <span className="ptl__walleticon"><WalletGlyph /></span> : null}
                  <span className="ptl__addr">{truncateAddress(profile.address)}</span>
                  {showCopy ? (
                    <button type="button" className="ptl__copy" aria-label="Copy address" onClick={onCopyAddress}>
                      <Copy size={16} />
                    </button>
                  ) : null}
                </div>
              </div>
            </div>

            {actions ? <div className="ptl__actions">{actions}</div> : null}
          </section>

          <nav className="ptl__tabs" aria-label={suffixLabel("Profile sections", labelSuffix)}>
            {tabs.map((tab) => {
              const isActive = tab.id === activeTab;
              return (
                <button
                  key={tab.id}
                  type="button"
                  className={"ptl__tab" + (isActive ? " is-active" : "")}
                  aria-current={isActive ? "page" : undefined}
                  onClick={onTabChange ? () => onTabChange(tab.id) : undefined}
                >
                  {tab.label}
                  {isActive ? <span className="ptl__tabbar" aria-hidden="true" /> : null}
                </button>
              );
            })}
          </nav>

          {aside ? (
            <div className="ptl__body ptl__body--split">
              <aside className="ptl__aside">{aside}</aside>
              <div className="ptl__main">{children}</div>
            </div>
          ) : (
            <div className={"ptl__body" + (bodyPadded ? " ptl__body--padded" : "")}>{children}</div>
          )}
        </div>
      </div>
    </div>
  );
}
