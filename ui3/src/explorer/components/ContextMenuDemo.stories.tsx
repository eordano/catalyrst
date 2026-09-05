import type { Meta, StoryObj } from "@storybook/react-vite";
import ContextMenu from "../../components/ContextMenu";

type MenuItem =
  | { kind: "caption"; label: string; avatar?: boolean; hue?: number }
  | { kind: "button"; label: string; to?: string; danger?: boolean }
  | { kind: "separator" };

const MENU: MenuItem[] = [
  { kind: "caption", label: "Alice.dcl", avatar: true, hue: 300 },
  { kind: "button", label: "Member" },
  { kind: "button", label: "View Profile", to: "Explorer/Pages/Passport" },
  { kind: "button", label: "Chat", to: "Explorer/Frames/Chat" },
  { kind: "button", label: "Call", to: "Explorer/Components/VoiceChat" },
  { kind: "button", label: "Gift" },
  { kind: "button", label: "Jump to Location", to: "Explorer/Workflows/SceneLoading" },
  { kind: "separator" },
  { kind: "button", label: "Report", danger: true },
  { kind: "button", label: "Block", danger: true, to: "Explorer/Components/Confirm" },
];

const meta = {
  title: "Explorer/Components/ContextMenuDemo",
  component: ContextMenu,
  parameters: { layout: "centered" },
} satisfies Meta<typeof ContextMenu>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <ContextMenu items={MENU} />,
};
