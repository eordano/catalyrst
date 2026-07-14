import { useEffect, useRef } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import MkSettingsPage from "@ui/marketplace/pages/MkSettingsPage";
import MkStoreSettingsEditor from "@ui/marketplace/pages/MkStoreSettingsEditor";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";
import "@ui/marketplace/pages/mksettingspage.css";
import "@ui/marketplace/pages/mkstoresettingseditor.css";

import {
  emptyAuthorizations,
  grantedCount,
  toSellingRows,
  type Authorizations,
  type Store,
} from "@data/lib/catalyst/marketplace/settings";
import { loadStore } from "@data/lib/catalyst/marketplace/settings.server";
import { useAuth } from "@data/lib/auth/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.settings";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/settings";

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

const TABS = ["authorizations", "store"] as const;
type Tab = (typeof TABS)[number];

function readTab(params: URLSearchParams): Tab {
  const raw = params.get("tab")?.trim() ?? "";
  return (TABS as readonly string[]).includes(raw) ? (raw as Tab) : "authorizations";
}

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_settings",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tab = readTab(url.searchParams);
  const address =
    url.searchParams.get("address")?.trim().toLowerCase() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { store, source, reason } = await loadStore(address, { signal: request.signal });

  const authorizations = emptyAuthorizations();

  const payload = {
    sid,
    tab,
    address,
    source,
    reason: reason ?? null,
    store,
    authorizations,
    granted: grantedCount(authorizations),
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  tab: Tab;
  address: string;
  source: "catalyst" | "empty" | "unavailable";
  reason: string | null;
  store: Store;
  authorizations: Authorizations;
  granted: number;
};

export default function MarketplaceSettings({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <SettingsSurface d={d} />;
}

function SettingsSurface({ d }: { d: LoaderData }) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const auth = useAuth();

  useSettingsViewed(d.sid, d);

  function onSignOut() {
    track("mk_settings_sign_out", {}, { sid: d.sid, story: STORY });
    auth.disconnect();
    navigate("/", { replace: true });
  }

  function selectTab(tab: Tab) {
    track("mk_settings_tab_changed", { tab }, { sid: d.sid, story: STORY });
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (tab === "authorizations") next.delete("tab");
        else next.set("tab", tab);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  return (
    <ChromeShell
      className="mk"
      ariaLabel="Settings"
      topbar={<DclTopBar variant="sites" active="shop" />}
      subnav={false}
    >
    <NewShopTabs tabs={SHOP_TABS} active="my-assets" />
    <div className="mk-settings">
      <nav className="mk-settings__tabs" aria-label="Settings sections" style={tabsStyle}>
        {TABS.map((t) => (
          <Link
            key={t}
            to={t === "authorizations" ? "/marketplace/settings" : `/marketplace/settings?tab=${t}`}
            onClick={(e) => {
              e.preventDefault();
              selectTab(t);
            }}
            aria-current={d.tab === t ? "page" : undefined}
            style={tabLinkStyle(d.tab === t)}
          >
            {TAB_LABELS[t]}
          </Link>
        ))}
        {auth.isConnected ? (
          <button
            type="button"
            onClick={onSignOut}
            style={signOutStyle}
            title="Sign out of this wallet on this device"
          >
            Sign out
          </button>
        ) : null}
      </nav>

      {d.tab === "authorizations" ? (
        <MkSettingsPage
          chrome={false}
          address={d.address}
          selling={toSellingRows(d.authorizations)}
        />
      ) : null}

      {d.tab === "store" ? (
        d.source === "unavailable" ? (
          <p role="alert" style={noticeStyle}>
            We couldn&apos;t load your store settings
            {d.reason ? ` (${d.reason})` : ""}. The form is hidden because blank
            fields here would look like your settings, and saving them could
            overwrite a store we simply failed to read. Reload in a moment.
          </p>
        ) : (
          <StoreEditorPane sid={d.sid} store={d.store} />
        )
      ) : null}
    </div>
    </ChromeShell>
  );
}

function StoreEditorPane({ sid, store }: { sid: string; store: Store }) {
  const touched = useRef<Set<string>>(new Set());
  function onChangeCapture(e: React.FormEvent<HTMLDivElement>) {
    const el = e.target as HTMLElement;
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      const field = fieldLabel(el);
      if (touched.current.has(field)) return;
      touched.current.add(field);
      track("mk_store_field_edited", { field }, { sid, story: STORY });
    }
  }

  function onClickCapture(e: React.MouseEvent<HTMLDivElement>) {
    const btn = (e.target as HTMLElement).closest("button.mkss__btn--primary");
    if (btn && !(btn as HTMLButtonElement).disabled) {
      track("mk_store_save_clicked", { deferred: true }, { sid, story: STORY });
    }
  }

  return (
    <div onChangeCapture={onChangeCapture} onClickCapture={onClickCapture}>
      <MkStoreSettingsEditor chrome={false} store={store} coverSize={undefined} />
    </div>
  );
}

function fieldLabel(el: HTMLInputElement | HTMLTextAreaElement): string {
  const field = el.closest(".mkss__field");
  const title = field?.querySelector(".mkss__ftitle")?.textContent?.trim();
  return title || (el.tagName === "TEXTAREA" ? "Description" : "field");
}

const TAB_LABELS: Record<Tab, string> = {
  authorizations: "Authorizations",
  store: "Store Settings",
};

const tabsStyle: React.CSSProperties = {
  display: "flex",
  gap: 8,
  alignItems: "center",
  padding: "10px 16px",
  background: "#0d0c11",
  borderBottom: "1px solid rgba(255,255,255,0.08)",
  flexWrap: "wrap",
};

function tabLinkStyle(active: boolean): React.CSSProperties {
  return {
    padding: "6px 14px",
    borderRadius: 999,
    fontSize: 14,
    fontWeight: 600,
    textDecoration: "none",
    color: active ? "#0d0c11" : "rgba(255,255,255,0.78)",
    background: active ? "#ff2d55" : "rgba(255,255,255,0.06)",
  };
}

const noticeStyle: React.CSSProperties = {
  margin: "24px 16px",
  padding: "16px 18px",
  borderRadius: 12,
  border: "1px solid rgba(255,45,85,0.4)",
  background: "rgba(255,45,85,0.08)",
  color: "rgba(255,255,255,0.86)",
  fontSize: 15,
  lineHeight: 1.5,
  maxWidth: 720,
};

const signOutStyle: React.CSSProperties = {
  marginLeft: "auto",
  padding: "6px 14px",
  borderRadius: 999,
  fontSize: 14,
  fontWeight: 600,
  border: "1px solid rgba(255,255,255,0.18)",
  background: "transparent",
  color: "rgba(255,255,255,0.78)",
  cursor: "pointer",
};

function useSettingsViewed(sid: string, d: LoaderData) {
  const last = useRef<string | null>(null);
  useEffect(() => {
    const key = `${d.source}|${d.tab}`;
    if (last.current === key) return;
    last.current = key;
    track(
      "mk_settings_viewed",
      { granted: d.granted, source: d.source, tab: d.tab },
      { sid, story: STORY },
    );
  }, [sid, d.source, d.tab, d.granted]);
}
