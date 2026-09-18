import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, waitFor, within } from "storybook/test";
import { loadMonaco } from "./monaco-host";
import DeCodeWorkspace from "./DeCodeWorkspace";

const meta = {
  title: "Editor/Code/Workspace",
  component: DeCodeWorkspace,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DeCodeWorkspace>;
export default meta;
type Story = StoryObj<typeof meta>;

export const TypescriptProject: Story = {
  args: { code: { virtualFiles: [{ path: "src/index.ts", text: "export const answer = 42;\n" }] } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => {
      expect(canvasElement.querySelector(".monaco-editor .view-lines")?.textContent).toContain("answer");
    }, { timeout: 20000 });
    expect(canvas.queryByText(/Editor failed to load/)).not.toBeInTheDocument();
    expect(canvas.getByTitle("src/index.ts")).toBeInTheDocument();
    const { monaco } = await loadMonaco();
    await waitFor(async () => {
      const worker = await (await monaco.typescript.getTypeScriptWorker())(monaco.Uri.parse("file:///src/index.ts"));
      await expect(worker.getSyntacticDiagnostics("file:///src/index.ts")).resolves.toEqual([]);
    }, { timeout: 20000 });
  },
};
