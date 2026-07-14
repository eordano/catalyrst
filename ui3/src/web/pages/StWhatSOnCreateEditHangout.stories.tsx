import type { Meta, StoryObj } from "@storybook/react-vite";
import StWhatSOnCreateEditHangout from "./StWhatSOnCreateEditHangout";

const MODES = ["create", "edit"] as const;
const STATES = ["form", "signin", "success"] as const;

const meta = {
  title: "Web/Pages/What's On/Create-Edit Hangout",
  component: StWhatSOnCreateEditHangout,
  parameters: { layout: "fullscreen" },
  argTypes: {
    mode: {
      control: "inline-radio",
      options: MODES,
      description: "`create` starts from an empty event, `edit` preloads an existing one.",
    },
    state: {
      control: "inline-radio",
      options: STATES,
      description: "Which body the page renders: the form, the sign-in gate, or the success screen.",
    },
  },
  args: { mode: "create", state: "form" },
} satisfies Meta<typeof StWhatSOnCreateEditHangout>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { key: string; mode: (typeof MODES)[number]; state: (typeof STATES)[number] }[] = [
  { key: "create-form", mode: "create", state: "form" },
  { key: "edit-form", mode: "edit", state: "form" },
  { key: "create-success", mode: "create", state: "success" },
  { key: "signin", mode: "create", state: "signin" },
];

/**
 * Every mode/state combination rendered at once. `Default` flips between them with the `mode`
 * and `state` controls; this story keeps all four in the render + a11y + visual-diff gates,
 * since each is a structurally different body (empty form, prefilled form, success screen,
 * sign-in gate).
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="st ui2" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {CATALOG.map(({ key, mode, state }) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={key}>
          <StWhatSOnCreateEditHangout mode={mode} state={state} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
