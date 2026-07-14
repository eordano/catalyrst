import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import ContextMenu from "./ContextMenu";

const meta = {
  tags: ["autodocs"],
  title: "Components/ContextMenu",
  component: ContextMenu,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ContextMenu>;

export default meta;
type Story = StoryObj<typeof meta>;

const Backdrop = ({ children }: { children: ReactNode }) => (
  <div
    style={{
      minHeight: "100vh",
      background: "#0d0c0f",
      display: "flex",
      alignItems: "flex-start",
      justifyContent: "center",
      padding: 48,
    }}
  >
    {children}
  </div>
);

export const Default: Story = {
  render: () => (
    <Backdrop>
      <ContextMenu
        items={[
          { kind: "title", label: "Alice.dcl" },
          { kind: "separator" },
          { kind: "button", label: "Mention" },
          { kind: "button", label: "View Profile" },
          { kind: "button", label: "Chat" },
          { kind: "button", label: "Call" },
          { kind: "button", label: "Gift" },
          { kind: "button", label: "Jump to Location" },
          { kind: "separator" },
          { kind: "button", label: "Report", danger: true },
          { kind: "button", label: "Block", danger: true },
        ]}
      />
    </Backdrop>
  ),
};

export const WithProfileHeader: Story = {
  render: () => (
    <Backdrop>
      <ContextMenu
        items={[
          {
            kind: "header",
            name: "vrhermit",
            tag: "#a1b2",
            address: "0x1234abcd5678ef901234abcd5678ef901234abcd",
            hue: 280,
          },
          { kind: "separator" },
          { kind: "button", label: "View Profile", icon: "👤" },
          { kind: "button", label: "Send Message", icon: "✉️" },
          { kind: "toggle", label: "Mute", checked: false },
          { kind: "submenu", label: "More Options", icon: "⚙️" },
          { kind: "separator" },
          { kind: "button", label: "Block User", icon: "🚫", danger: true },
        ]}
      />
    </Backdrop>
  ),
};

export const SimpleButtons: Story = {
  render: () => (
    <Backdrop>
      <ContextMenu
        items={[
          { kind: "button", label: "Cut", icon: "✂️" },
          { kind: "button", label: "Copy", icon: "📋" },
          { kind: "button", label: "Paste", icon: "📌" },
          { kind: "separator" },
          { kind: "button", label: "Delete", icon: "🗑️", danger: true },
        ]}
      />
    </Backdrop>
  ),
};

export const WithTogglesAndCaption: Story = {
  render: () => (
    <Backdrop>
      <ContextMenu
        items={[
          { kind: "caption", label: "Notifications", avatar: true, hue: 200 },
          { kind: "toggle", label: "Mentions", checked: true },
          { kind: "toggle", label: "Direct Messages", checked: false },
          { kind: "separator" },
          { kind: "submenu", label: "Advanced", icon: "⚙️" },
        ]}
      />
    </Backdrop>
  ),
};
