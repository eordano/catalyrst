import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StHelpSupportCenter, { HelpTab, Status, SERVICES } from "./StHelpSupportCenter";

type StHelpSupportCenterProps = ComponentProps<typeof StHelpSupportCenter>;

const SERVICE_SETS = {
  none: undefined,
  allOk: SERVICES,
  degraded: SERVICES.map((s, i) => (i === 3 || i === 8 ? { ...s, status: Status.DOWN } : s)),
  allDown: SERVICES.map((s) => ({ ...s, status: Status.DOWN })),
};
type ServicesKey = keyof typeof SERVICE_SETS;
const SERVICE_KEYS = Object.keys(SERVICE_SETS) as ServicesKey[];

const TABS = [HelpTab.FAQ, HelpTab.SUPPORT_UPDATES];

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

export const Default: Story = {};

export const SupportUpdates: Story = { args: { activeTab: HelpTab.SUPPORT_UPDATES } };

export const StatusLoading: Story = { args: { servicesPreset: "none", statusLoading: true } };

export const StatusDegraded: Story = { args: { servicesPreset: "degraded" } };

export const StatusDown: Story = { args: { servicesPreset: "allDown" } };
