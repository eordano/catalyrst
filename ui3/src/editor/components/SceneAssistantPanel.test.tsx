import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { SceneAssistant } from "../types";
import SceneAssistantPanel from "./SceneAssistantPanel";

afterEach(cleanup);

function fixture(): SceneAssistant {
  return {
    conversations: vi.fn().mockResolvedValue({ conversations: [] }),
    conversation: vi.fn(),
    deleteConversation: vi.fn().mockResolvedValue(undefined),
    providers: vi.fn().mockResolvedValue({
      providers: [{ id: "codex", label: "Codex", available: true }, { id: "gemini", label: "Gemini", available: true }],
      sceneTools: { available: true, paired: false, url: "http://localhost:8080/mcp" }, busy: false,
    }),
    turn: vi.fn<SceneAssistant["turn"]>().mockImplementation(async (_input, emit) => {
      emit({ type: "started", turnId: "turn-1", provider: "codex", conversationId: "conversation-1" });
      emit({ type: "session", sessionId: "session-1" });
      emit({ type: "text", text: "Added " });
      emit({ type: "text", text: "a tree." });
      emit({ type: "done", exitCode: 0, cancelled: false });
    }),
    cancel: vi.fn().mockResolvedValue(undefined),
  };
}

async function send(text: string) {
  fireEvent.change(await screen.findByRole("textbox", { name: "Describe the scene change" }), { target: { value: text } });
  const button = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
}

it("streams replies, resumes the conversation, and clears provider-specific sessions on switching", async () => {
  const assistant = fixture();
  render(<SceneAssistantPanel assistant={assistant} onClose={vi.fn()} />);
  await send("Add a tree");
  expect(await screen.findByText("Added a tree.")).toBeInTheDocument();
  expect(screen.getByText(/Pair the editor tab/)).toBeInTheDocument();
  await send("Make it larger");
  await waitFor(() => expect(assistant.turn).toHaveBeenCalledTimes(2));
  expect(vi.mocked(assistant.turn).mock.calls[1]![0].conversationId).toBe("conversation-1");
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Provider" })).toBeEnabled());
  fireEvent.change(screen.getByRole("combobox", { name: "Provider" }), { target: { value: "gemini" } });
  await send("Add a rock");
  await waitFor(() => expect(assistant.turn).toHaveBeenCalledTimes(3));
  expect(vi.mocked(assistant.turn).mock.calls[2]![0].conversationId).toBeUndefined();
});

it("stops the active SDK turn and aborts its stream", async () => {
  const assistant = fixture();
  let signal: AbortSignal | undefined;
  vi.mocked(assistant.turn).mockImplementation((_input, emit, requestSignal) => {
    signal = requestSignal;
    emit({ type: "started", turnId: "active-turn", provider: "codex" });
    return new Promise((resolve) => requestSignal.addEventListener("abort", () => resolve(), { once: true }));
  });
  render(<SceneAssistantPanel assistant={assistant} onClose={vi.fn()} />);
  await send("Add a tree");
  fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
  expect(assistant.cancel).toHaveBeenCalledWith("active-turn");
  expect(signal?.aborted).toBe(true);
  await waitFor(() => expect(screen.getByRole("button", { name: "Close" })).toBeEnabled());
});

it("keeps the actionable provider error when the process subsequently exits", async () => {
  const assistant = fixture();
  vi.mocked(assistant.turn).mockImplementation(async (_input, emit) => {
    emit({ type: "error", message: "Sign in to Codex on the SDK machine" });
    emit({ type: "error", message: "Assistant exited with status Some(1); check CLI authentication and permissions" });
    emit({ type: "done", exitCode: 1, cancelled: false });
  });
  render(<SceneAssistantPanel assistant={assistant} onClose={vi.fn()} />);
  await send("Add a tree");
  expect(await screen.findByRole("alert")).toHaveTextContent("Sign in to Codex on the SDK machine");
});

it("pairs this editor through the SDK bridge and refreshes the confirmed scene status", async () => {
  const assistant = fixture();
  const before = await assistant.providers();
  before.sceneTools.bridge = "ws://localhost:8000/api/project/assistant/bridge";
  vi.mocked(assistant.providers).mockResolvedValueOnce(before)
    .mockResolvedValue({ ...before, sceneTools: { ...before.sceneTools, paired: true } });
  const onPair = vi.fn().mockResolvedValue(undefined);
  render(<SceneAssistantPanel assistant={assistant} onClose={vi.fn()} onPair={onPair} />);
  fireEvent.click(await screen.findByRole("button", { name: "Pair this editor" }));
  expect(onPair).toHaveBeenCalledWith(before.sceneTools.bridge);
  expect(await screen.findByText(/Connected to scene tools/)).toBeInTheDocument();
});


it("loads SDK-owned history after reopening and resumes with current selected entities", async () => {
  const assistant = fixture();
  const saved = { id: "saved-chat", provider: "codex", title: "Make a forest", updatedAt: 123, truncated: false };
  vi.mocked(assistant.conversations).mockResolvedValue({ conversations: [saved] });
  vi.mocked(assistant.conversation).mockResolvedValue({ ...saved, resumable: true, events: [
    { type: "user", text: "Plant trees", selectedEntities: [] }, { type: "text", text: "Trees planted." },
  ] });
  const first = render(<SceneAssistantPanel assistant={assistant} onClose={vi.fn()} />);
  await screen.findByRole("option", { name: "Make a forest" });
  first.unmount();
  render(<SceneAssistantPanel assistant={assistant} selectedEntities={[{ id: "42", name: "Oak" }]} onClose={vi.fn()} />);
  await screen.findByRole("option", { name: "Make a forest" });
  fireEvent.change(screen.getByRole("combobox", { name: "Conversation" }), { target: { value: "saved-chat" } });
  expect(await screen.findByText("Trees planted.")).toBeInTheDocument();
  expect(screen.getByLabelText("Selected entity context")).toHaveTextContent("Oak (#42)");
  await send("Make this tree larger");
  await waitFor(() => expect(assistant.turn).toHaveBeenCalled());
  expect(vi.mocked(assistant.turn).mock.calls[0]![0]).toEqual({ provider: "codex", prompt: "Make this tree larger", conversationId: "saved-chat", selectedEntities: [{ id: "42", name: "Oak" }] });
});
