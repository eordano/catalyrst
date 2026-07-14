import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChAppSettingsTabbedSections from "./ChAppSettingsTabbedSections";

const INVALID_PATH_ERROR =
  "Folder doesn't exist or can't save files there. Please choose a different folder.";

const meta = {
  title: "CreatorHub/Components/App Settings: tabbed sections",
  component: ChAppSettingsTabbedSections,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    open: { control: "boolean" },
    web: { control: "boolean" },
    initialTab: { control: "inline-radio", options: ["scenes", "editor", "about"] },
    updateState: {
      control: "select",
      options: ["idle", "checking", "downloading", "available", "downloaded", "up_to_date"],
      description: "Auto-update state shown on the About tab.",
    },
    progress: { control: { type: "range", min: 0, max: 100 } },
    initialError: { control: "text", description: "Scenes-tab folder validation error." },
    releaseNotes: { control: "object", description: "`null` hides the release-notes block." },
    version: { control: "text" },
  },
  args: {
    variant: "panel",
    open: true,
    initialTab: "scenes",
    updateState: "idle",
    progress: 0,
    initialError: null,
  },
  // `initialTab` / `initialError` seed internal state, so remount when a control changes them.
  render: (args) => (
    <ChAppSettingsTabbedSections key={`${args.initialTab}|${args.initialError}`} {...args} />
  ),
} satisfies Meta<typeof ChAppSettingsTabbedSections>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type AsProps = ComponentProps<typeof ChAppSettingsTabbedSections>;

/**
 * Ordering matters. The dialog title is an `h6` and the release notes render `h3`s, so any entry
 * that shows notes must be the last one on the page -- otherwise the next entry's `h6` is a
 * three-level jump and axe's document-scoped heading-order fires. `available` therefore drops its
 * notes (`releaseNotes: null`) and `downloaded` keeps them, last.
 */
const CASES: { label: string; args: Partial<AsProps> }[] = [
  { label: "Scenes tab", args: { initialTab: "scenes" } },
  { label: "Editor tab", args: { initialTab: "editor" } },
  { label: "About tab \u{B7} idle", args: { initialTab: "about", updateState: "idle" } },
  { label: "About tab \u{B7} up to date", args: { initialTab: "about", updateState: "up_to_date" } },
  {
    label: "About tab \u{B7} downloading",
    args: { initialTab: "about", updateState: "downloading", progress: 42 },
  },
  {
    label: "Scenes tab \u{B7} invalid folder",
    args: { initialTab: "scenes", initialError: INVALID_PATH_ERROR },
  },
  {
    label: "About tab \u{B7} update available",
    args: { initialTab: "about", updateState: "available", releaseNotes: null },
  },
  { label: "About tab \u{B7} downloaded (with release notes)", args: { initialTab: "about", updateState: "downloaded" } },
];

export const Catalog: Story = {
  name: "Catalog (every tab and update state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChAppSettingsTabbedSections {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
