import type { Meta, StoryObj } from "@storybook/react-vite";
import MkStoreSettingsEditor from "./MkStoreSettingsEditor";

const meta = {
  title: "Marketplace/Pages/Store Settings editor",
  component: MkStoreSettingsEditor,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof MkStoreSettingsEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = {
  args: {
    store: {
      owner: "0x9f3c\u{2026}7a21",
      cover: "",
      coverName: "",
      description: "",
      website: "",
      facebook: "",
      twitter: "",
      discord: "",
    },
  },
};

export const Loading: Story = {
  args: { isLoading: true },
};

export const Saving: Story = {
  args: { isSaving: true },
};

export const Error: Story = {
  args: { error: "Could not save your store. Please try again." },
};
