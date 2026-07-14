import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";
import AssetTopbar from "./AssetTopbar";

type TopbarProps = ComponentProps<typeof AssetTopbar>;

const GridGlyph = (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <rect x="3" y="3" width="7" height="7" rx="1.2" />
    <rect x="14" y="3" width="7" height="7" rx="1.2" />
    <rect x="3" y="14" width="7" height="7" rx="1.2" />
    <rect x="14" y="14" width="7" height="7" rx="1.2" />
  </svg>
);
const AtlasGlyph = (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" />
  </svg>
);

/** Which sort menu the topbar is given -- `none` drops the sort control entirely. */
const SORTS = {
  collectibles: [
    { id: "recently_listed", label: "Recently listed" },
    { id: "newest", label: "Newest" },
    { id: "cheapest", label: "Cheapest" },
    { id: "most_expensive", label: "Most expensive" },
    { id: "recently_sold", label: "Recently sold" },
  ],
  land: [
    { id: "newest", label: "Newest" },
    { id: "cheapest", label: "Cheapest" },
    { id: "most_expensive", label: "Most expensive" },
    { id: "name", label: "Name" },
    { id: "size", label: "Size" },
  ],
  none: undefined,
} satisfies Record<string, TopbarProps["sortOptions"]>;

/** `default` uses the component's built-in view list; `none` drops the view toggle. */
const VIEWS = {
  default: undefined,
  gridAtlas: [
    { id: "grid", label: "Grid view", icon: GridGlyph },
    { id: "atlas", label: "Atlas map view", icon: AtlasGlyph },
  ],
  none: undefined,
} satisfies Record<string, TopbarProps["viewOptions"]>;

type SortKey = keyof typeof SORTS;
type ViewKey = keyof typeof VIEWS;

type TopbarStoryArgs = {
  layout: NonNullable<TopbarProps["layout"]>;
  searchPlaceholder: string;
  count: number;
  showCount: boolean;
  sortLabel: string;
  sorts: SortKey;
  views: ViewKey;
};

/** Keeps sort/view interactive the way the pre-collapse stories did, seeded from the args. */
function TopbarDemo({ layout, searchPlaceholder, count, showCount, sortLabel, sorts, views }: TopbarStoryArgs) {
  const sortOptions = SORTS[sorts];
  const [sort, setSort] = useState(sortOptions?.[0]?.id);
  const [view, setView] = useState("grid");
  useEffect(() => setSort(sortOptions?.[0]?.id), [sortOptions]);
  return (
    <AssetTopbar
      layout={layout}
      searchPlaceholder={searchPlaceholder}
      count={count}
      showCount={showCount}
      sortLabel={sortLabel}
      sortOptions={sortOptions}
      sort={sort}
      onSort={setSort}
      viewOptions={VIEWS[views]}
      view={views === "none" ? undefined : view}
      onView={setView}
    />
  );
}

const meta = {
  title: "Marketplace/Components/AssetTopbar",
  component: AssetTopbar,
  parameters: { layout: "padded" },
  argTypes: {
    layout: { control: "inline-radio", options: ["inline", "stacked"] },
    searchPlaceholder: { control: "text" },
    count: { control: "number" },
    showCount: { control: "boolean" },
    sortLabel: { control: "text" },
    sorts: { control: "select", options: ["collectibles", "land", "none"] },
    views: { control: "select", options: ["default", "gridAtlas", "none"] },
  },
  args: {
    layout: "inline",
    searchPlaceholder: "Search collectibles",
    count: 12,
    showCount: true,
    sortLabel: "",
    sorts: "collectibles",
    views: "default",
  },
  render: (args) => <TopbarDemo {...args} />,
} satisfies Meta<TopbarStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<TopbarStoryArgs> }[] = [
  { label: "Collectibles \u{B7} inline", args: {} },
  {
    label: "LAND \u{B7} stacked, atlas toggle",
    args: { layout: "stacked", searchPlaceholder: "Search Land...", sorts: "land", views: "gridAtlas" },
  },
  { label: "Single result", args: { count: 1 } },
  {
    label: "Search + count only",
    args: { searchPlaceholder: "Search", count: 248, sorts: "none", views: "none" },
  },
];

/**
 * Every layout at once. `Default` flips between them from the Controls panel; this keeps all four
 * in the render + a11y + visual-diff gates, since dropping the sort/view controls and the stacked
 * layout are structurally different subtrees.
 */
export const Catalog: Story = {
  name: "Catalog (every layout)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 28 }}>
      {CASES.map((c) => (
        <section key={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <TopbarDemo {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
