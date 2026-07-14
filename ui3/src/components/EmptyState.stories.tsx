import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import EmptyState from "./EmptyState";
import EmptyStateCard from "./EmptyStateCard";

const SearchGlyph = (
  <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="7" />
    <path d="M21 21l-4.3-4.3" />
  </svg>
);
const HouseGlyph = (
  <svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 9l9-6 9 6v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V9Z" />
    <path d="M9 21V12h6v9" />
  </svg>
);
const CubeGlyph = (
  <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 7l9-4 9 4-9 4-9-4Z" />
    <path d="M3 7v10l9 4 9-4V7" />
    <path d="M12 11v10" />
  </svg>
);
const Watermelon = (
  <svg viewBox="0 0 48 48" width="44" height="44" fill="none" aria-hidden="true">
    <path d="M6 12a18 18 0 0 0 36 0Z" fill="#44b600" />
    <path d="M9 13a15 15 0 0 0 30 0Z" fill="#fff" opacity=".55" />
    <path d="M11 13.5a13 13 0 0 0 26 0Z" fill="#ff5f87" />
    <circle cx="19" cy="22" r="1.3" fill="#161518" />
    <circle cx="24" cy="25" r="1.3" fill="#161518" />
    <circle cx="29" cy="22" r="1.3" fill="#161518" />
  </svg>
);
const WarningGlyph = (
  <svg viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 8v5M12 16.5v.01" />
    <circle cx="12" cy="12" r="9" />
  </svg>
);
const NoCameraGlyph = (
  <svg viewBox="0 0 120 120" width="120" height="120" fill="none" aria-hidden="true">
    <rect x="20" y="38" width="80" height="52" rx="8" stroke="#fff" strokeWidth="4" />
    <circle cx="60" cy="64" r="16" stroke="#fff" strokeWidth="4" />
    <path d="M42 38l6-10h24l6 10" stroke="#fff" strokeWidth="4" strokeLinejoin="round" />
    <path d="M14 14l92 92" stroke="#fff" strokeWidth="4" strokeLinecap="round" />
  </svg>
);

const ICON = {
  none: undefined,
  search: SearchGlyph,
  house: HouseGlyph,
  cube: CubeGlyph,
  warning: WarningGlyph,
  watermelon: Watermelon,
  camera: NoCameraGlyph,
};

const SUBTITLE = {
  none: undefined,
  filters: "Try adjusting your filters or search terms.",
  owned: "Items you own will show up here.",
  clearFilters: "Try clearing the filters to see all available LAND.",
  createList: "Create a list to start saving your favourite wearables and emotes.",
  searchTerm: 'No results found for "neon jacket"',
  retry: "Please try again.",
  photoGone:
    "Whoops! The photo you are trying to access does not exist or is no longer available.",
  rich: (
    <>
      Unleash your creativity. Start building scenes for your LANDs and Worlds and share with the
      community.{" "}
      <a
        href="https://docs.decentraland.org/creator/scenes-sdk7/getting-started/sdk-101"
        target="_blank"
        rel="noopener noreferrer"
      >
        Learn more about creating Scenes.
      </a>
    </>
  ),
};

const ACTIONS = {
  none: undefined,
  resetOutline: [{ label: "Reset filters", variant: "outline" }],
  createList: [{ label: "Create list", variant: "solid" }],
  tryAgain: [{ label: "Try again", variant: "solid" }],
  viewAll: [{ label: "View all Projects", variant: "solid" }],
  custom: (
    <button type="button" className="es__cta">
      + New scene
    </button>
  ),
} as const;

const meta = {
  tags: ["autodocs"],
  title: "Components/EmptyState",
  component: EmptyState,
  parameters: { layout: "padded" },
  argTypes: {
    icon: { control: "select", options: Object.keys(ICON), mapping: ICON },
    iconWash: { control: "boolean" },
    title: { control: "text" },
    titleAs: { control: "select", options: ["h1", "h2", "h3", "p"] },
    subtitle: { control: "select", options: Object.keys(SUBTITLE), mapping: SUBTITLE },
    actions: { control: "select", options: Object.keys(ACTIONS), mapping: ACTIONS },
    variant: { control: "select", options: ["inline", "screen"] },
    tone: { control: "select", options: ["error"] },
    actionsGap: { control: "number" },
  },
  args: {
    icon: "search",
    iconWash: true,
    title: "No collectibles found",
    subtitle: "filters",
    actions: "none",
  },
} satisfies Meta<typeof EmptyState>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const cell: CSSProperties = {
  border: "1px solid rgba(255,255,255,.12)",
  borderRadius: 12,
  padding: 16,
  minWidth: 320,
  flex: "1 1 340px",
};

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 16 }}>
      <div style={cell}>
        <EmptyState
          icon={SearchGlyph}
          iconWash
          title="No collectibles found"
          subtitle={SUBTITLE.filters}
        />
      </div>
      <div style={cell}>
        <EmptyState icon={CubeGlyph} title="No assets yet" subtitle={SUBTITLE.owned} />
      </div>
      <div style={cell}>
        <EmptyState
          icon={HouseGlyph}
          title="No results found for these filters."
          subtitle={SUBTITLE.clearFilters}
          actions={[{ label: "Reset filters", variant: "outline" }]}
        />
      </div>
      <div style={cell}>
        <EmptyState
          icon={CubeGlyph}
          title="You don't have any lists yet"
          subtitle={SUBTITLE.createList}
          actions={[{ label: "Create list", variant: "solid" }]}
        />
      </div>
      <div style={cell}>
        <EmptyState title="Nothing to show" subtitle={SUBTITLE.searchTerm} />
      </div>
      <div style={cell}>
        <EmptyState
          title="Create your first scene"
          subtitle={SUBTITLE.rich}
          actions={ACTIONS.custom}
        />
      </div>
      <div style={cell}>
        <EmptyState variant="inline" title="No friends" />
      </div>
      <div style={cell}>
        <EmptyState
          tone="error"
          icon={WarningGlyph}
          title="Oops! Lists couldn't load."
          subtitle={SUBTITLE.retry}
          actions={[{ label: "Try again", variant: "solid" }]}
        />
      </div>
    </div>
  ),
};

const screenStyle: CSSProperties & { "--es-screen-bg": string; "--es-screen-h": string } = {
  "--es-screen-bg": "#242129",
  "--es-screen-h": "420px",
};

export const ScreenCover: Story = {
  parameters: { layout: "fullscreen", controls: { disable: true } },
  render: () => (
    <EmptyState
      variant="screen"
      style={screenStyle}
      icon={NoCameraGlyph}
      title="Photo not found"
      subtitle={SUBTITLE.photoGone}
    />
  ),
};

export const Card: Story = {
  render: (args) => <EmptyStateCard {...args} />,
  args: {
    icon: "watermelon",
    iconWash: false,
    title: "Looks like there are no Projects following these criteria to be displayed",
    subtitle: "none",
    actions: "viewAll",
  },
};
