import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StCastWatcher from "./StCastWatcher";

type WatcherProps = ComponentProps<typeof StCastWatcher>;
type WatcherToasts = NonNullable<WatcherProps["toasts"]>;

const PLAYBACK_ERROR: WatcherToasts = [
  {
    title: "Video couldn't play",
    message: "We couldn't start playback. Click retry to try again.",
    action: "Retry",
  },
];

/** The toast stack is picked by name; `none` is the quiet page. */
const TOASTS = { none: [] as WatcherToasts, playbackError: PLAYBACK_ERROR };
type ToastName = keyof typeof TOASTS;

/** Story args: the toast stack is picked by name, everything else is a real prop. */
type WatcherStoryArgs = Omit<WatcherProps, "toasts"> & { toastPreset: ToastName };

const meta = {
  title: "Web/Pages/Cast/Watcher",
  component: StCastWatcher,
  parameters: { layout: "fullscreen" },
  argTypes: {
    state: { control: "select", options: ["onboarding", "joining", "live", "waiting"] },
    streamName: { control: "text" },
    sidebarOpen: { control: "boolean" },
    isTabMuted: { control: "boolean" },
    unreadCount: { control: "number" },
    participantCount: { control: "number" },
    toastPreset: {
      control: "inline-radio",
      options: ["none", "playbackError"],
      description: "Which `toasts` stack is rendered over the player.",
    },
  },
  args: {
    state: "live",
    streamName: "Genesis Plaza",
    sidebarOpen: true,
    unreadCount: 3,
    toastPreset: "none",
  },
  render: ({ toastPreset, ...rest }) => <StCastWatcher toasts={TOASTS[toastPreset]} {...rest} />,
} satisfies Meta<WatcherStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state rendered at once. `Default` flips between them with the `state` /
 * `sidebarOpen` / `toastPreset` controls; this story keeps the sidebar and fullscreen
 * live layouts, the onboarding card, the joining spinner, the nobody-is-casting waiting
 * screen and the playback-error toast in the render + a11y + visual-diff gates.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="st ui2" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {/* <section> demotes each entry's unnamed header/footer/aside to `generic`
          (HTML-AAM scoped mapping) so the stack does not invent extra landmarks. */}
      <section>
        <div>live, sidebar open</div>
        <StCastWatcher state="live" sidebarOpen unreadCount={3} chrome={false} />
      </section>
      <section>
        <div>live, fullscreen</div>
        <StCastWatcher state="live" sidebarOpen={false} chrome={false} />
      </section>
      <section>
        <div>onboarding</div>
        <StCastWatcher state="onboarding" streamName="Genesis Plaza" chrome={false} />
      </section>
      <section>
        <div>joining</div>
        <StCastWatcher state="joining" chrome={false} />
      </section>
      <section>
        <div>waiting</div>
        <StCastWatcher state="waiting" sidebarOpen={false} chrome={false} />
      </section>
      <section>
        <div>live with toast</div>
        <StCastWatcher state="live" sidebarOpen={false} toasts={PLAYBACK_ERROR} chrome={false} />
      </section>
    </div>
  ),
};
