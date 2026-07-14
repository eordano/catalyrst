import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StStorageEnvironment from "./StStorageEnvironment";
import type { EnvKey, Scope } from "./StStorageEnvironment";

const ENV_KEYS: EnvKey[] = [
  { key: "API_BASE_URL" },
  { key: "OPENAI_API_KEY" },
  { key: "ANALYTICS_WRITE_KEY" },
  { key: "FEATURE_FLAGS" },
  { key: "WEBHOOK_SECRET" },
];

const SCOPE: Scope = { realm: "vitsky.dcl.eth", position: "0,0" };
const REALM_ONLY_SCOPE: Scope = { realm: "buenosaires.dcl.eth", position: null };

/** The key list is picked by name: `populated` is the demo set, `empty` the zero-state. */
const KEY_SETS = { populated: ENV_KEYS, empty: [] as EnvKey[] };
type KeySetName = keyof typeof KEY_SETS;

/** The scope descriptor is picked by name. */
const SCOPES = { realmAndPosition: SCOPE, realmOnly: REALM_ONLY_SCOPE };
type ScopeName = keyof typeof SCOPES;

type EnvironmentProps = ComponentProps<typeof StStorageEnvironment>;

/** Story args: key list and scope are picked by name, everything else is a real prop. */
type EnvironmentStoryArgs = Omit<EnvironmentProps, "envKeys" | "scope"> & {
  keySet: KeySetName;
  scopePreset: ScopeName;
};

const meta = {
  title: "Web/Pages/Storage/Environment",
  component: StStorageEnvironment,
  parameters: { layout: "fullscreen" },
  argTypes: {
    keySet: {
      control: "inline-radio",
      options: ["populated", "empty"],
      description: "Which `envKeys` list is rendered \u{2014} `empty` is the zero-state.",
    },
    scopePreset: {
      control: "inline-radio",
      options: ["realmAndPosition", "realmOnly"],
      description: "Which `scope` descriptor drives the header label.",
    },
    isLoading: { control: "boolean" },
    activeTab: { control: "select", options: ["env", "scene", "players"] },
    embedded: { control: "boolean" },
  },
  args: { keySet: "populated", scopePreset: "realmAndPosition", isLoading: false },
  render: ({ keySet, scopePreset, ...rest }) => (
    <StStorageEnvironment envKeys={KEY_SETS[keySet]} scope={SCOPES[scopePreset]} {...rest} />
  ),
} satisfies Meta<EnvironmentStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state rendered at once. `Default` flips between them with the `keySet` /
 * `scopePreset` / `isLoading` controls; this story keeps the populated table, the empty
 * state, the loading skeleton and the realm-only header in the render + a11y + visual-diff gates.
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
        <div>populated</div>
        <StStorageEnvironment envKeys={ENV_KEYS} scope={SCOPE} chrome={false} />
      </section>
      <section>
        <div>empty</div>
        <StStorageEnvironment envKeys={[]} scope={SCOPE} chrome={false} />
      </section>
      <section>
        <div>loading</div>
        <StStorageEnvironment isLoading scope={SCOPE} chrome={false} />
      </section>
      <section>
        <div>realm-only scope</div>
        <StStorageEnvironment envKeys={ENV_KEYS} scope={REALM_ONLY_SCOPE} chrome={false} />
      </section>
    </div>
  ),
};
