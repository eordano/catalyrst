import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChWorldSettingsTabbedSections, {
  type WorldSceneVM,
} from "./ChWorldSettingsTabbedSections";

const WORLD_NAME = "mystore.dcl.eth";

const SAMPLE_SCENES: WorldSceneVM[] = [
  { entityId: "s1", title: "Main Plaza", parcelCount: 4, coord: "0, 0", canUnpublish: true },
  { entityId: "s2", title: "Wearable Gallery", parcelCount: 2, coord: "2, 0", canUnpublish: true },
  { entityId: "s3", title: "Rooftop Stage", parcelCount: 6, coord: "0, 2", canUnpublish: false },
];

const meta = {
  title: "CreatorHub/Components/World Settings tabs",
  component: ChWorldSettingsTabbedSections,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    tab: { control: "inline-radio", options: ["details", "layout", "general"] },
    layoutView: {
      control: "select",
      options: ["scenes", "map", "empty", "unpublish"],
      description: "Sub-view of the Layout tab; ignored on the other tabs.",
    },
    isOwner: { control: "boolean" },
    isLoading: { control: "boolean" },
    hasChanges: { control: "boolean" },
    worldName: { control: "text" },
    scenes: { control: "object", description: "An empty array renders the empty-world layout." },
  },
  args: {
    variant: "panel",
    worldName: WORLD_NAME,
    scenes: SAMPLE_SCENES,
    tab: "details",
    layoutView: "scenes",
    isOwner: true,
    isLoading: false,
    hasChanges: false,
  },
  // `tab` seeds internal state, so remount when the control changes it.
  render: (args) => <ChWorldSettingsTabbedSections key={args.tab} {...args} />,
} satisfies Meta<typeof ChWorldSettingsTabbedSections>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type WsProps = ComponentProps<typeof ChWorldSettingsTabbedSections>;

// The dialog exposes `role="region"` named `World Settings - {worldName}`, so every entry needs
// its own world name or axe's landmark-unique fires once per repeat.
const CASES: { label: string; args: Partial<WsProps> }[] = [
  { label: "Details tab", args: { tab: "details", worldName: "details.dcl.eth" } },
  { label: "Layout tab \u{B7} scenes", args: { tab: "layout", worldName: "layout.dcl.eth" } },
  { label: "General tab", args: { tab: "general", worldName: "general.dcl.eth" } },
  {
    label: "Layout \u{B7} world map",
    args: { tab: "layout", layoutView: "map", worldName: "map.dcl.eth" },
  },
  {
    label: "Details \u{B7} unsaved changes",
    args: { tab: "details", hasChanges: true, worldName: "unsaved.dcl.eth" },
  },
  {
    label: "Layout \u{B7} empty world",
    args: { tab: "layout", layoutView: "empty", scenes: [], worldName: "empty.dcl.eth" },
  },
  {
    label: "Layout \u{B7} unpublish confirmation",
    args: { tab: "layout", layoutView: "unpublish", worldName: "unpublish.dcl.eth" },
  },
  { label: "Collaborator (not owner)", args: { isOwner: false, worldName: "shared.dcl.eth" } },
  {
    label: "Loading",
    args: { tab: "details", isLoading: true, worldName: "loading.dcl.eth" },
  },
];

export const Catalog: Story = {
  name: "Catalog (every tab and view)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChWorldSettingsTabbedSections {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
