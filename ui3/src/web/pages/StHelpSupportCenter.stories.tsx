import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StHelpSupportCenter, { HelpTab, Status, SERVICES } from "./StHelpSupportCenter";

type StHelpSupportCenterProps = ComponentProps<typeof StHelpSupportCenter>;

/** The service-status fixtures each former variant story passed. */
const SERVICE_SETS = {
  none: undefined,
  allOk: SERVICES,
  degraded: SERVICES.map((s, i) => (i === 3 || i === 8 ? { ...s, status: Status.DOWN } : s)),
  allDown: SERVICES.map((s) => ({ ...s, status: Status.DOWN })),
};
type ServicesKey = keyof typeof SERVICE_SETS;
const SERVICE_KEYS = Object.keys(SERVICE_SETS) as ServicesKey[];

const TABS = [HelpTab.FAQ, HelpTab.SUPPORT_UPDATES];

/** Story args: the service list is picked by name, everything else is a real prop. */
type HelpStoryArgs = Omit<StHelpSupportCenterProps, "services"> & { servicesPreset: ServicesKey };

const meta = {
  title: "Web/Pages/Help & Support Center",
  component: StHelpSupportCenter,
  parameters: { layout: "fullscreen" },
  argTypes: {
    servicesPreset: {
      control: "select",
      options: SERVICE_KEYS,
      description: "Which status fixture the sidebar reports; `none` passes no services at all.",
    },
    activeTab: { control: "inline-radio", options: TABS },
    statusLoading: { control: "boolean" },
    supportEmail: { control: "text" },
  },
  args: { servicesPreset: "allOk", activeTab: HelpTab.FAQ, statusLoading: false },
  render: ({ servicesPreset, ...rest }) => (
    <StHelpSupportCenter services={SERVICE_SETS[servicesPreset]} {...rest} />
  ),
} satisfies Meta<HelpStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

/*
 * `Default` and `SupportUpdates` keep their own exports -- thin arg presets over the collapsed
 * `meta` -- because `tools/screen-tour/add-story-links.mts` deep-links their story ids
 * (`web-pages-help-support-center--default` / `--support-updates`) and a missing id only
 * console.warns. The three status stories they used to sit beside are collapsed into the
 * `servicesPreset`/`statusLoading` controls; the `Catalog` below keeps those states gated.
 */

export const Default: Story = {};

export const SupportUpdates: Story = { args: { activeTab: HelpTab.SUPPORT_UPDATES } };

/** The status sidebar's spinner. */
export const StatusLoading: Story = { args: { servicesPreset: "none", statusLoading: true } };

/** Two services down. */
export const StatusDegraded: Story = { args: { servicesPreset: "degraded" } };

/** Every service down. */
export const StatusDown: Story = { args: { servicesPreset: "allDown" } };
