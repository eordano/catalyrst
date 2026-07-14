import type { CSSProperties } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { linkTo } from "@storybook/addon-links";
import type { Decorator, Preview } from "@storybook/react-vite";
import { initialize, mswLoader } from "msw-storybook-addon";
import "./preview.css";

const SW_URL =
  typeof location !== "undefined" && /\/iframe\.html$/.test(location.pathname)
    ? new URL("mockServiceWorker.js", location.href).pathname
    : "/mockServiceWorker.js";
initialize({ onUnhandledRequest: "bypass", serviceWorker: { url: SW_URL } });

const VIEWPORTS = {
  desktop: { name: "Desktop", styles: { width: "1440px", height: "900px" }, type: "desktop" },
  mobile: { name: "Mobile", styles: { width: "390px", height: "844px" }, type: "mobile" },
  tablet: { name: "Tablet", styles: { width: "768px", height: "1024px" }, type: "tablet" },
} as const;

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30 * 1000,
      gcTime: 5 * 60 * 1000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});
const withQueryClient: Decorator = (Story) => (
  <QueryClientProvider client={queryClient}>
    <Story />
  </QueryClientProvider>
);

const withExperienceLinks: Decorator = (Story) => {
  const onClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as Element | null;
    const el = target?.closest?.("[data-sb-linkto]");
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    const dest = el.getAttribute("data-sb-linkto");
    if (dest) linkTo(dest)();
  };
  return (
    <div style={{ display: "contents" }} onClick={onClick}>
      <Story />
    </div>
  );
};

const SCRIM_STYLE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0, 0, 0, .502)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: 24,
};
const withOverlayScrim: Decorator = (Story, ctx) => {
  const tagged = ctx.tags?.includes("overlay") || ctx.parameters?.overlay;
  if (!tagged) return <Story />;
  return (
    <div style={SCRIM_STYLE}>
      <Story />
    </div>
  );
};

const withSceneBackdrop: Decorator = (Story, ctx) => {
  const tagged = ctx.tags?.includes("scene-backdrop") || ctx.parameters?.sceneBackdrop;
  if (!tagged) return <Story />;
  return (
    <>
      <div className="scene-bd" aria-hidden="true">
        <div className="scene-bd__rail" />
        <div className="scene-bd__minimap" />
        <div className="scene-bd__chat" />
      </div>
      <Story />
    </>
  );
};

const UI2_NAMESPACES = [
  "Account/",
  "Marketplace/",
  "Governance/",
  "Sites/",
  "CreatorHub/",
];
const UI2_PAGE_TITLES = new Set([
  "Web/Pages/Donations",
  "Web/Workflows/Gifting",
  "Web/Pages/MarketplaceCredits",
  "Web/Pages/MarketplaceUnlocked",
  "Web/Pages/CreditsStates",
  "Web/Workflows/Login",
  "Web/Workflows/Otp",
  "Web/Workflows/Verify",
  "Web/Workflows/Web3Confirm",
]);
const withDappScope: Decorator = (Story, ctx) => {
  const t = ctx.title || "";
  const scoped =
    ctx.tags?.includes("ui2") ||
    ctx.parameters?.ui2 ||
    UI2_PAGE_TITLES.has(t) ||
    UI2_NAMESPACES.some((ns) => t.startsWith(ns));
  if (!scoped) return <Story />;
  return (
    <div className="ui2" style={{ display: "contents" }}>
      <Story />
    </div>
  );
};

const withTheme: Decorator = (Story, ctx) => {
  if (typeof document !== "undefined") {
    const light = ctx.globals.theme === "light";
    document.documentElement.classList.toggle("theme-light", light);
  }
  return <Story />;
};

const preview = {
  loaders: [mswLoader],
  decorators: [
    withQueryClient,
    withExperienceLinks,
    withTheme,
    withDappScope,
    withOverlayScrim,
    withSceneBackdrop,
  ],
  globalTypes: {
    theme: {
      description: "App theme — Dark (default, app-consistent) / Light (faithful upstream)",
      toolbar: {
        title: "Theme",
        icon: "contrast",
        items: [
          { value: "dark", title: "Theme: Dark" },
          { value: "light", title: "Theme: Light" },
        ],
        dynamicTitle: true,
      },
    },
  },
  parameters: {
    layout: "fullscreen",
    a11y: { test: "error" },
    viewport: { options: VIEWPORTS },
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
    options: {
      storySort: {
        order: [
          "Atoms", "Components",
          "Web", "Explorer", "Marketplace", "CreatorHub", "*",
        ],
      },
    },
  },
  initialGlobals: {
    theme: "dark",
    viewport: { value: "desktop", isRotated: false },
  },
} satisfies Preview;

export default preview;
