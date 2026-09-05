import type { Meta, StoryObj } from "@storybook/react-vite";
import CliEscape from "./CliEscape";

const meta = {
  title: "CreatorHub/Components/CliEscape",
  component: CliEscape,
  parameters: {
    docs: {
      description: {
        component:
          "The escape hatch from an unbuilt panel: the command that really works, copyable. There is deliberately **no Run button** \u{2014} the browser cannot reach `dcl-one-sdk` or an explorer's `--mcp` port, so a Run control could only report a success it did not cause.",
      },
    },
  },
  argTypes: {
    command: { control: "text" },
    explain: { control: "text" },
    docs: { control: "text" },
  },
  args: {
    command: "dcl-one-sdk world permissions grant petbarn.dcl.eth deployment 0x313d\u{2026}9a1",
    explain:
      "PUT /world/{name}/permissions/deployment/{address} over an EIP-191 signed chain.",
  },
  render: (args) => (
    <div style={{ maxWidth: 560 }}>
      <CliEscape {...args} />
    </div>
  ),
} satisfies Meta<typeof CliEscape>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithDocs: Story = {
  args: { docs: "/create/learn" },
};

/** Multi-line commands keep their line breaks and scroll rather than wrap. */
export const TwoStep: Story = {
  args: {
    command:
      "# start your explorer with --mcp yourself first \u{2014} this does not launch it\ncurl -s http://127.0.0.1:8123/unity-explorer-mcp -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"get_scene_logs\",\"arguments\":{}}}'",
    explain:
      "dcl-scene-bots is an MCP client you run beside your own explorer. The browser cannot reach that port, which is why there is no button here.",
  },
};
