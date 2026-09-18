import { useEffect, useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, waitFor } from "storybook/test";
import DeWorkspace from "./DeWorkspace";

function BridgeFixture({ fail = false }: { fail?: boolean }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    const html = `<!doctype html><html><body style="background:#252529;color:white;font:16px sans-serif;padding:24px">
      Simulated engine \u2014 testing the browser/scene bridge
      <script>
      window.engine_console_command = async () => '';
      const session = new URL(location.href).searchParams.get('editorSession');
      const channel = new BroadcastChannel('dcl-editor-bus:' + session);
      const send = msg => channel.postMessage({ to: 'page', msg });
      channel.onmessage = ({ data }) => {
        if (data.to !== 'scene') return;
        const msg = data.msg;
        if (msg.type === 'init') send({ type: 'scene-ready', scene: {
          hash: 'fixture', title: 'Bridge fixture', parcels: [], isPortable: false,
          isBroken: false, isBlocked: false, isSuper: false, sdkVersion: '7'
        }, frozen: false, tool: 'translate', orientGlobal: false, pivotEach: false, selected: [], active: null });
        if (msg.type === 'rpc') send({ type: 'rpc-reply', id: msg.id,
          ok: ${!fail}, result: msg.method === 'exportComposite' ? '{"components":[]}' : null,
          ${fail ? "error: 'Simulated restore failure'" : "error: undefined"} });
      };
      </script></body></html>`;
    const url = URL.createObjectURL(new Blob([html], { type: "text/html" }));
    setSrc(url + '?editorSession=00000000-0000-4000-8000-000000000001');
    return () => URL.revokeObjectURL(url);
  }, [fail]);
  return <DeWorkspace title="Bridge fixture" viewportSrc={src} rawComposite={'{"components":[]}'} />;
}

const meta = {
  title: "Editor/Pages/Workspace bridge",
  component: BridgeFixture,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof BridgeFixture>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Connected: Story = {
  args: {},
  play: async ({ canvasElement, canvas }) => {
    await waitFor(() => expect(canvasElement.querySelector("iframe")).not.toBeNull());
    await waitFor(() => expect(canvasElement.querySelector(".eui-boot:not(.is-leaving)")).toBeNull(), { timeout: 10000 });
    expect(canvasElement.querySelector('[role="tablist"]')).not.toBeNull();
    await userEvent.click(canvas.getByRole("button", { name: "Play" }));
    await userEvent.click(await canvas.findByRole("button", { name: "Pause preview" }));
    await userEvent.click(await canvas.findByRole("button", { name: "Resume preview" }));
    await canvas.findByRole("button", { name: "Pause preview" });
    await userEvent.click(canvas.getByRole("button", { name: "Stop preview" }));
    await canvas.findByRole("button", { name: "Play" });
    const iframe = canvasElement.querySelector("iframe")!;
    const loaded = new Promise<void>((resolve) => iframe.addEventListener("load", () => resolve(), { once: true }));
    iframe.src = iframe.src;
    await loaded;
    await waitFor(() => expect(canvasElement.querySelector(".eui-boot:not(.is-leaving)")).toBeNull(), { timeout: 10000 });
  },
};

export const RestoreFailed: Story = {
  args: { fail: true },
  play: async ({ canvas }) => {
    expect(await canvas.findByRole("alert", {}, { timeout: 10000 })).toHaveTextContent("could not be loaded");
    expect(canvas.getByRole("button", { name: "Retry" })).toBeEnabled();
  },
};
