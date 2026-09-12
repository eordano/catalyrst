import type { Meta, StoryObj } from "@storybook/react-vite";
import AuthLayout from "./AuthLayout";

const PANEL = {
  signIn: (
    <>
      <h1>Sign in to Decentraland</h1>
      <p>Sign in to enter the world.</p>
    </>
  ),
  loading: <h1>Loading your avatar&#x2026;</h1>,
  verify: <h1>Verify your account</h1>,
  minimal: <h1>Minimal shell</h1>,
};

const TOP_LEFT = { none: undefined, back: <button>&#x2190; Back</button> };
const BOTTOM_LEFT = { none: undefined, version: <span>editor version</span> };

const meta = {
  title: "Web/Frames/AuthLayout",
  component: AuthLayout,
  parameters: { layout: "fullscreen" },
  argTypes: {
    children: {
      control: "select",
      options: Object.keys(PANEL),
      mapping: PANEL,
      description: "Which panel subtree fills the shell.",
    },
    topLeft: { control: "select", options: Object.keys(TOP_LEFT), mapping: TOP_LEFT },
    bottomLeft: { control: "select", options: Object.keys(BOTTOM_LEFT), mapping: BOTTOM_LEFT },
    centered: { control: "boolean" },
    hideBrand: { control: "boolean" },
    hideFooter: { control: "boolean" },
    brandGlyph: { control: "boolean" },
  },
  args: {
    children: "signIn",
    topLeft: "none",
    bottomLeft: "none",
    centered: false,
    hideBrand: false,
    hideFooter: false,
    brandGlyph: false,
  },
} satisfies Meta<typeof AuthLayout>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {/* <section> demotes each entry's unnamed header/footer/aside to `generic`
          (HTML-AAM scoped mapping) so the stack does not invent extra landmarks. */}
      <section>
        <AuthLayout>{PANEL.signIn}</AuthLayout>
      </section>
      <section>
        <AuthLayout centered>{PANEL.loading}</AuthLayout>
      </section>
      <section>
        <AuthLayout topLeft={TOP_LEFT.back} bottomLeft={BOTTOM_LEFT.version} brandGlyph>
          {PANEL.verify}
        </AuthLayout>
      </section>
      <section>
        <AuthLayout hideBrand hideFooter>
          {PANEL.minimal}
        </AuthLayout>
      </section>
    </div>
  ),
};
