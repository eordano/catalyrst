import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StCastStreamer from "./StCastStreamer";
import type { Toast } from "./StCastStreamer";

const DEMO_TOASTS: Toast[] = [
  {
    id: "ss",
    title: "Screen sharing failed",
    message: "Your screen share stopped. Click retry to share again.",
    action: { label: "Retry", onClick: () => {} },
  },
];

/** The toast stack is picked by name; `none` is the clean page. */
const TOASTS = { none: [] as Toast[], screenShareFailed: DEMO_TOASTS };
type ToastName = keyof typeof TOASTS;

type StreamerProps = ComponentProps<typeof StCastStreamer>;

/** Story args: the toast stack is picked by name, everything else is a real prop. */
type StreamerStoryArgs = Omit<StreamerProps, "toasts"> & { toastPreset: ToastName };

const meta = {
  title: "Web/Pages/Cast/Streamer",
  component: StCastStreamer,
  parameters: { layout: "fullscreen" },
  argTypes: {
    state: { control: "select", options: ["onboarding", "joining", "live", "error"] },
    streamName: { control: "text" },
    displayName: { control: "text" },
    unreadMessages: { control: "number" },
    participants: { control: "number" },
    tabMuted: { control: "boolean" },
    errorTitle: { control: "text" },
    errorMessage: { control: "text" },
    retryLabel: { control: "text" },
    leaveLabel: { control: "text" },
    toastPreset: {
      control: "inline-radio",
      options: ["none", "screenShareFailed"],
      description: "Which `toasts` stack is rendered over the page.",
    },
  },
  args: {
    state: "live",
    streamName: "Genesis Plaza",
    displayName: "ruby.dcl.eth",
    unreadMessages: 2,
    participants: 1,
    toastPreset: "screenShareFailed",
  },
  render: ({ toastPreset, ...rest }) => <StCastStreamer toasts={TOASTS[toastPreset]} {...rest} />,
} satisfies Meta<StreamerStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state rendered at once. `Default` flips between them with the `state` /
 * `toastPreset` controls; this story keeps the live page with and without a toast,
 * the onboarding card, the joining spinner and the connection-error screen in the
 * render + a11y + visual-diff gates.
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
        <div>live with toast</div>
        <StCastStreamer
          state="live"
          displayName="ruby.dcl.eth"
          unreadMessages={2}
          participants={1}
          toasts={DEMO_TOASTS}
          chrome={false}
        />
      </section>
      <section>
        <div>onboarding</div>
        <StCastStreamer state="onboarding" streamName="Genesis Plaza" toasts={[]} chrome={false} />
      </section>
      <section>
        <div>joining</div>
        <StCastStreamer state="joining" toasts={[]} chrome={false} />
      </section>
      <section>
        <div>connection error</div>
        <StCastStreamer state="error" toasts={[]} chrome={false} />
      </section>
      <section>
        <div>live, clean</div>
        <StCastStreamer
          state="live"
          displayName="ruby.dcl.eth"
          unreadMessages={2}
          participants={1}
          toasts={[]}
          chrome={false}
        />
      </section>
    </div>
  ),
};
