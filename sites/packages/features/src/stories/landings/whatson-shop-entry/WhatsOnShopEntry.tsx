import { useEffect, useRef } from "react";

import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";
import {
  WHATSON_SHOP_ENTRY_TARGETS,
  whatsOnShopItemPath,
  type WhatsOnShopEntryArm,
} from "@core/lib/experiments/whatson-shop-entry";
import {
  track as defaultTrack,
  type TrackContext,
  type TrackFn,
} from "@core/lib/telemetry/track";

export type WhatsOnShopOpenTarget = "pill" | "rail_cta" | "rail_item";

export type WhatsOnShopEntryProps = {
  arm: WhatsOnShopEntryArm;
  /** Rail reading from the live catalog; null = reading unavailable. */
  items: CollectibleCard[] | null;
  trackCtx: TrackContext;
  track?: TrackFn;
  navigate?: (url: string) => void;
};

function defaultNavigate(url: string): void {
  if (typeof window !== "undefined") window.location.assign(url);
}

export default function WhatsOnShopEntry({
  arm,
  items,
  trackCtx,
  track = defaultTrack,
  navigate = defaultNavigate,
}: WhatsOnShopEntryProps) {
  const shownRef = useRef(false);
  useEffect(() => {
    if (arm === "base" || shownRef.current) return;
    shownRef.current = true;
    track("lp_whatson_shop_entry_shown", { variant: arm }, trackCtx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (arm === "base") return null;

  const open = (
    target: WhatsOnShopOpenTarget,
    itemId: null | string,
    url: string,
  ) => {
    track(
      "lp_whatson_shop_opened",
      { item_id: itemId, target, variant: arm },
      trackCtx,
    );
    navigate(url);
  };

  if (arm === "pill") {
    return (
      <div className="wo-shop-entry" data-variant="pill" style={PILL_ROW_STYLE}>
        <a
          href={WHATSON_SHOP_ENTRY_TARGETS.shop}
          className="whatson-route__pill"
          style={{ textDecoration: "none" }}
          onClick={(e) => {
            e.preventDefault();
            open("pill", null, WHATSON_SHOP_ENTRY_TARGETS.shop);
          }}
        >
          Dress for tonight -- Shop emotes &amp; wearables
        </a>
      </div>
    );
  }

  return (
    <section
      className="wo-shop-entry"
      data-variant="rail"
      aria-label="Gear up before you go"
      style={RAIL_STYLE}
    >
      <div style={RAIL_HEAD_STYLE}>
        <p style={EYEBROW_STYLE}>Gear up before you go</p>
        <h2 style={RAIL_TITLE_STYLE}>Fresh emotes on sale</h2>
        {items === null ? (
          <p style={NOTE_STYLE}>Live listings are unavailable right now.</p>
        ) : null}
      </div>
      {items?.length ? (
        <ul style={RAIL_LIST_STYLE}>
          {items.map((c) => (
            <li key={c.id} style={{ listStyle: "none" }}>
              <a
                href={whatsOnShopItemPath(c.id)}
                style={ITEM_STYLE}
                onClick={(e) => {
                  e.preventDefault();
                  open("rail_item", c.id, whatsOnShopItemPath(c.id));
                }}
              >
                {c.image ? (
                  <img src={c.image} alt="" width={56} height={56} style={ITEM_IMG_STYLE} />
                ) : (
                  <span style={{ ...ITEM_IMG_STYLE, background: "rgba(255,255,255,0.12)" }} aria-hidden="true" />
                )}
                <span style={ITEM_NAME_STYLE}>{c.name}</span>
                {c.credits != null || c.price != null ? (
                  <span style={ITEM_PRICE_STYLE}>
                    {c.credits ?? c.price} {c.credits != null ? "credits" : "MANA"}
                  </span>
                ) : null}
              </a>
            </li>
          ))}
        </ul>
      ) : null}
      <a
        href={WHATSON_SHOP_ENTRY_TARGETS.shop}
        style={CTA_STYLE}
        onClick={(e) => {
          e.preventDefault();
          open("rail_cta", null, WHATSON_SHOP_ENTRY_TARGETS.shop);
        }}
      >
        Open the Shop
      </a>
    </section>
  );
}

const PILL_ROW_STYLE: React.CSSProperties = {
  display: "flex",
  margin: "0 0 12px",
};
const RAIL_STYLE: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  flexWrap: "wrap",
  gap: 16,
  background: "rgba(255, 255, 255, 0.08)",
  border: "1px solid rgba(255, 255, 255, 0.14)",
  borderRadius: 14,
  padding: "14px 18px",
  margin: "0 0 14px",
};
const RAIL_HEAD_STYLE: React.CSSProperties = {
  flex: "0 0 auto",
  minWidth: 160,
};
const EYEBROW_STYLE: React.CSSProperties = {
  margin: 0,
  fontSize: 11,
  fontWeight: 700,
  letterSpacing: "1px",
  textTransform: "uppercase",
  color: "rgba(255, 255, 255, 0.65)",
};
const RAIL_TITLE_STYLE: React.CSSProperties = {
  margin: "2px 0 0",
  fontSize: 18,
  fontWeight: 800,
  color: "#fff",
};
const NOTE_STYLE: React.CSSProperties = {
  margin: "4px 0 0",
  fontSize: 12,
  color: "rgba(255, 255, 255, 0.7)",
};
const RAIL_LIST_STYLE: React.CSSProperties = {
  display: "flex",
  flexWrap: "wrap",
  gap: 10,
  flex: "1 1 auto",
  margin: 0,
  padding: 0,
};
const ITEM_STYLE: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  background: "rgba(255, 255, 255, 0.08)",
  border: "1px solid rgba(255, 255, 255, 0.12)",
  borderRadius: 10,
  padding: "8px 12px 8px 8px",
  color: "#fff",
  textDecoration: "none",
};
const ITEM_IMG_STYLE: React.CSSProperties = {
  display: "block",
  width: 56,
  height: 56,
  borderRadius: 8,
  objectFit: "cover",
};
const ITEM_NAME_STYLE: React.CSSProperties = {
  fontSize: 13,
  fontWeight: 700,
  maxWidth: 140,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};
const ITEM_PRICE_STYLE: React.CSSProperties = {
  fontSize: 12,
  fontWeight: 700,
  color: "rgba(255, 255, 255, 0.8)",
  whiteSpace: "nowrap",
};
const CTA_STYLE: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  flex: "0 0 auto",
  marginLeft: "auto",
  padding: "10px 20px",
  borderRadius: 999,
  background: "linear-gradient(180deg, #ff8a3d 0%, #ff6a2c 100%)",
  color: "#fff",
  textDecoration: "none",
  fontWeight: 700,
  fontSize: 13,
  boxShadow: "0 1px 6px rgba(255, 122, 61, 0.35)",
};
