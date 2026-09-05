import type { Meta, StoryObj } from "@storybook/react-vite";
import ChModalMobileQRCode from "./ChModalMobileQRCode";
import type { MobileDebugSession } from "./ChModalMobileQRCode";

const PREVIEW_URL = "http://192.168.1.42:8000/?realm=LocalPreview&position=0,0";

/** The session list is picked by name: `none` is the waiting state. */
const SESSION_SETS = {
  none: [] as MobileDebugSession[],
  one: [{ id: 1, messageCount: 1284 }],
  two: [
    { id: 1, messageCount: 1284 },
    { id: 2, messageCount: 57 },
  ],
};
type SessionSetName = keyof typeof SESSION_SETS;

/** Story args: the session list is picked by fixture name, the rest are real props. */
type QrStoryArgs = {
  sessionSet: SessionSetName;
  open: boolean;
  url: string;
  simulateLive: boolean;
};

const meta = {
  title: "CreatorHub/Components/Mobile QR Code",
  component: ChModalMobileQRCode,
  parameters: { layout: "fullscreen" },
  argTypes: {
    sessionSet: {
      control: "inline-radio",
      options: ["none", "one", "two"],
      description: "Which connected-session list the dialog lists \u{2014} `none` is the waiting state.",
    },
    open: { control: "boolean", description: "`false` unmounts the dialog; nothing is rendered." },
    url: { control: "text", description: "The preview URL the QR code encodes, printed below it." },
    simulateLive: {
      control: "boolean",
      description:
        "Fakes a device connecting: with no sessions the dialog flips from waiting to one session 1.6s after mount. Timer-driven, so this case lives here rather than in `Catalog`, which has to be deterministic for the screenshot baseline.",
    },
  },
  args: { sessionSet: "none", open: true, url: PREVIEW_URL, simulateLive: false },
  // `sessions` is latched into useState and `simulateLive`'s timer only arms on mount, so both
  // controls would look dead without a key that remounts the component when they change.
  render: ({ sessionSet, ...rest }) => (
    <ChModalMobileQRCode
      key={`${sessionSet}-${rest.simulateLive}-${rest.open}`}
      sessions={SESSION_SETS[sessionSet]}
      {...rest}
    />
  ),
} satisfies Meta<QrStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; sessionSet: SessionSetName }[] = [
  { label: "waiting for a device", sessionSet: "none" },
  { label: "one session connected", sessionSet: "one" },
  { label: "multiple sessions", sessionSet: "two" },
];

/**
 * Every session state at once. This is possible because `Modal` takes `portal={false}`, which
 * lays the same card out in normal document flow instead of `createPortal`ing a
 * `position: fixed; inset: 0` backdrop onto `document.body` -- portalled dialogs stack on one
 * another, so a single screenshot would capture only the topmost. The dialog owns no chrome, and
 * its heading id is `useId()`-generated, so three copies on one page do not collide. The
 * `simulateLive` and dismissed (`open: false`) cases are reachable from the controls on
 * `Default`; `simulateLive` is deliberately not a catalog entry because its 1.6s timer would make
 * the shared screenshot baseline non-deterministic.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 24, padding: 24 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <ChModalMobileQRCode
            portal={false}
            url={PREVIEW_URL}
            sessions={SESSION_SETS[entry.sessionSet]}
          />
        </section>
      ))}
    </div>
  ),
};
