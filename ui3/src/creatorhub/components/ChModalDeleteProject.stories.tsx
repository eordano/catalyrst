import type { Meta, StoryObj } from "@storybook/react-vite";
import ChModalDeleteProject from "./ChModalDeleteProject";

const PROJECTS = {
  neonPlaza: {
    id: "5f2a1c44-9d3e-4b8a-bf21-7c0e0a1d4e90",
    title: "Neon Plaza",
    path: "~/dcl-scenes/neon-plaza",
  },
  sunsetGallery: {
    id: "a71f9b22-4c18-4e7d-9a03-2b6e1f0c8d44",
    title: "Sunset Gallery",
    path: "~/dcl-scenes/sunset-gallery",
  },
  longTitle: {
    id: "c0d3f00d-1234-5678-9abc-def012345678",
    title: "My Very Ambitious Multi-Parcel Festival Grounds Experience",
    path: "~/dcl-scenes/festival-grounds",
  },
};
type ProjectKey = keyof typeof PROJECTS;

/** Story args: the project is picked by fixture name, the rest are real props. */
type DeleteStoryArgs = {
  projectPreset: ProjectKey;
  open: boolean;
  deleteFiles: boolean;
  chrome?: boolean;
};

const meta = {
  title: "CreatorHub/Components/Delete Project",
  component: ChModalDeleteProject,
  parameters: { layout: "fullscreen" },
  argTypes: {
    projectPreset: {
      control: "select",
      options: ["neonPlaza", "sunsetGallery", "longTitle"],
      description: "Which project fixture the dialog is confirming against \u{2014} `longTitle` overflows the heading.",
    },
    open: { control: "boolean", description: "`false` dismisses the dialog and leaves only the chrome." },
    deleteFiles: {
      control: "boolean",
      description: "Pre-ticks the delete-files checkbox, which reveals the permanence warning.",
    },
  },
  args: { projectPreset: "neonPlaza", open: true, deleteFiles: false },
  // `deleteFiles` is latched into useState on mount, so the control would look dead without a
  // key that remounts the component when it changes.
  render: ({ projectPreset, deleteFiles, ...rest }) => (
    <ChModalDeleteProject
      key={`${projectPreset}-${deleteFiles}`}
      project={PROJECTS[projectPreset]}
      deleteFiles={deleteFiles}
      {...rest}
    />
  ),
} satisfies Meta<DeleteStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; projectPreset: ProjectKey; deleteFiles: boolean }[] = [
  { label: "default", projectPreset: "neonPlaza", deleteFiles: false },
  { label: "delete files checked", projectPreset: "sunsetGallery", deleteFiles: true },
  { label: "long title", projectPreset: "longTitle", deleteFiles: false },
];

/**
 * Every open state at once. This is possible because `Modal` takes `portal={false}`, which lays
 * the same card out in normal document flow instead of `createPortal`ing a `position: fixed`
 * backdrop onto `document.body` -- portalled dialogs stack on one another, so a single screenshot
 * would capture only the topmost. `chrome={false}` keeps the stack from emitting N `<main>`
 * landmarks. The dismissed state is reachable from the `open` control on `Default`; it renders
 * only `CreatorHubChrome`, which that frame's own stories already gate.
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
          <ChModalDeleteProject
            chrome={false}
            portal={false}
            project={PROJECTS[entry.projectPreset]}
            deleteFiles={entry.deleteFiles}
          />
        </section>
      ))}
    </div>
  ),
};
