import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StStoragePlayers from "./StStoragePlayers";

const PLAYERS: string[] = [
  "0x6a77833d2b7f0c6c0e6c4a45a6f8e3c1d9b27a41",
  "0x8f2a5c9d0b1e4f7a3c6d8b2e5a9f0c3d7b1e6a82",
  "0x1b3c5d7e9f0a2c4e6d8b0a1c3e5f7d9b2a4c6e80",
  "0xc4e6a8b0d2f4a6c8e0b2d4f6a8c0e2b4d6f8a0c2",
  "0x3d5f7a9c1e3b5d7f9a1c3e5b7d9f1a3c5e7b9d10",
  "0x9a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a64",
  "0x5c7e9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c30",
  "0x2e4c6a8d0b2f4e6c8a0d2b4f6e8c0a2d4b6f8e09"
];

const PROFILE_NAMES = new Map<string, string>([
  ["0x6a77833d2b7f0c6c0e6c4a45a6f8e3c1d9b27a41", "BraveExplorer"],
  ["0x8f2a5c9d0b1e4f7a3c6d8b2e5a9f0c3d7b1e6a82", "NeonNomad"],
  ["0xc4e6a8b0d2f4a6c8e0b2d4f6a8c0e2b4d6f8a0c2", "pixel.dcl.eth"],
  ["0x9a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a64", "VoxelVagrant"],
  ["0x5c7e9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c30", "AuroraBuilder"]
]);

const NO_PROFILE_NAMES = new Map<string, string>();

/** The roster (addresses + their profile names) is picked by name. */
const ROSTERS = {
  populated: { players: PLAYERS, profileNames: PROFILE_NAMES },
  empty: { players: [] as string[], profileNames: NO_PROFILE_NAMES },
};
type RosterName = keyof typeof ROSTERS;

/** The page is scoped either to a realm or to a parcel position, never both. */
const SCOPES = {
  realm: { realm: "magma.dcl.eth", position: null },
  position: { realm: null, position: "-50,72" },
};
type ScopeName = keyof typeof SCOPES;

type PlayersProps = ComponentProps<typeof StStoragePlayers>;

/** Story args: roster and scope are picked by name, everything else is a real prop. */
type PlayersStoryArgs = Omit<PlayersProps, "players" | "profileNames" | "realm" | "position"> & {
  roster: RosterName;
  scope: ScopeName;
};

const meta = {
  title: "Web/Pages/Storage/Players",
  component: StStoragePlayers,
  parameters: { layout: "fullscreen" },
  argTypes: {
    roster: {
      control: "inline-radio",
      options: ["populated", "empty"],
      description: "Which player list (and matching profile names) is rendered.",
    },
    scope: {
      control: "inline-radio",
      options: ["realm", "position"],
      description: "`realm` scopes the page to a world; `position` scopes it to a parcel.",
    },
    isLoading: { control: "boolean" },
    embedded: { control: "boolean" },
  },
  args: { roster: "populated", scope: "realm", isLoading: false },
  render: ({ roster, scope, ...rest }) => (
    <StStoragePlayers {...SCOPES[scope]} {...ROSTERS[roster]} {...rest} />
  ),
} satisfies Meta<PlayersStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state rendered at once. `Default` flips between them with the `roster` / `scope` /
 * `isLoading` controls; this story keeps the populated roster, the loading skeleton, the
 * empty state and the position-scoped header in the render + a11y + visual-diff gates.
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
        <StStoragePlayers realm="magma.dcl.eth" players={PLAYERS} profileNames={PROFILE_NAMES} chrome={false} />
      </section>
      <section>
        <div>loading</div>
        <StStoragePlayers realm="magma.dcl.eth" isLoading chrome={false} />
      </section>
      <section>
        <div>empty</div>
        <StStoragePlayers realm="magma.dcl.eth" players={[]} profileNames={NO_PROFILE_NAMES} chrome={false} />
      </section>
      <section>
        <div>position-scoped</div>
        <StStoragePlayers
          realm={null}
          position="-50,72"
          players={PLAYERS}
          profileNames={PROFILE_NAMES}
          chrome={false}
        />
      </section>
    </div>
  ),
};
