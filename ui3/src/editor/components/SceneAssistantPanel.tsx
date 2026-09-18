import { useEffect, useRef, useState } from "react";
import type { SceneAssistant, AssistantConversation, AssistantHistoryEvent, AssistantSelection } from "../types";
import "./scene-assistant.css";

type Entry = { role: "user" | "assistant" | "tool"; text: string };

function transcript(events: AssistantHistoryEvent[]): Entry[] {
  const result: Entry[] = [];
  for (const event of events) {
    if (event.type === "user") result.push({ role: "user", text: event.text });
    else if (event.type === "text") {
      const tail = result.at(-1);
      if (tail?.role === "assistant") tail.text += event.text;
      else result.push({ role: "assistant", text: event.text });
    } else if (event.type === "tool") result.push({ role: "tool", text: `${event.name}: ${event.detail}` });
    else if (event.type === "error") result.push({ role: "tool", text: event.message });
  }
  return result;
}

export default function SceneAssistantPanel({ assistant, onClose, onPair, selectedEntities = [] }: { assistant: SceneAssistant; onClose: () => void; onPair?: (url: string) => Promise<void>; selectedEntities?: AssistantSelection[] }) {
  const [providers, setProviders] = useState<Awaited<ReturnType<SceneAssistant["providers"]>> | null>(null);
  const [provider, setProvider] = useState("");
  const [history, setHistory] = useState<AssistantConversation[]>([]);
  const [conversationId, setConversationId] = useState<string | undefined>();
  const [loadingHistory, setLoadingHistory] = useState(true);
  const [truncated, setTruncated] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [busy, setBusy] = useState(false);
  const [pairing, setPairing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  const turnId = useRef<string | null>(null);
  useEffect(() => {
    let dead = false;
    void Promise.all([assistant.providers(), assistant.conversations()]).then(([result, saved]) => {
      if (dead) return;
      setProviders(result);
      setProvider(result.providers.find((item) => item.available)?.id || "");
      setHistory(saved.conversations);
      setLoadingHistory(false);
    }).catch((error: unknown) => { if (!dead) { setError(String(error)); setLoadingHistory(false); } });
    return () => { dead = true; request.current?.abort(); };
  }, [assistant]);
  const send = async () => {
    if (busy || loadingHistory || !provider || !prompt.trim()) return;
    const controller = new AbortController();
    request.current = controller;
    turnId.current = null;
    setBusy(true);
    setError(null);
    const text = prompt.trim();
    setPrompt("");
    setEntries((previous) => [...previous, { role: "user", text }]);
    try {
      await assistant.turn({ provider, prompt: text, ...(conversationId ? { conversationId } : {}), selectedEntities }, (event) => {
        if (controller.signal.aborted) return;
        if (event.type === "started") { turnId.current = event.turnId; if (event.conversationId) setConversationId(event.conversationId); }
        else if (event.type === "text") setEntries((previous) => {
          const tail = previous.at(-1);
          return tail?.role === "assistant"
            ? [...previous.slice(0, -1), { role: "assistant", text: tail.text + event.text }]
            : [...previous, { role: "assistant", text: event.text }];
        });
        else if (event.type === "tool") setEntries((previous) => [...previous, { role: "tool", text: `${event.name}: ${event.detail}` }]);
        else if (event.type === "error") setError(previous => previous ?? event.message);
        else if (event.type === "done" && event.exitCode !== 0 && !event.cancelled) setError((previous) => previous ?? `The assistant exited with code ${event.exitCode ?? "unknown"}. Check its sign-in and try again.`);
      }, controller.signal);
    } catch (error) {
      if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false); request.current = null; turnId.current = null;
      void assistant.conversations().then(saved => setHistory(saved.conversations)).catch(error => setError(String(error)));
    }
  };
  return <section className="eui-assistant" aria-label="Scene assistant">
    <header>
      <h2>Scene assistant</h2>
      <button type="button" className="eui-btn" onClick={onClose} disabled={busy || pairing || loadingHistory}>Close</button>
    </header>
    <label>Conversation
      <select className="eui-select" value={conversationId ?? ""} disabled={busy || loadingHistory} onChange={(event) => {
        const id = event.target.value;
        if (!id) { setConversationId(undefined); setEntries([]); setTruncated(false); return; }
        setLoadingHistory(true); setError(null);
        void assistant.conversation(id).then(saved => {
          setConversationId(saved.id); setProvider(saved.provider); setEntries(transcript(saved.events)); setTruncated(saved.truncated);
        }).catch(error => setError(String(error))).finally(() => setLoadingHistory(false));
      }}>
        <option value="">New conversation</option>
        {history.map(item => <option key={item.id} value={item.id}>{item.title}</option>)}
      </select>
    </label>
    {conversationId && <button type="button" className="eui-btn" disabled={busy || loadingHistory} onClick={() => {
      setLoadingHistory(true); setError(null);
      void assistant.deleteConversation(conversationId).then(() => {
        setConversationId(undefined); setEntries([]); setTruncated(false);
        return assistant.conversations();
      }).then(saved => setHistory(saved.conversations)).catch(error => setError(String(error))).finally(() => setLoadingHistory(false));
    }}>Delete conversation</button>}
    {truncated && <p role="status">This conversation exceeded the saved history limit. Some messages were not saved.</p>}
    {conversationId && provider === "gemini" && <p role="status">Gemini history is saved, but this provider starts each turn without resuming prior model context.</p>}
    {selectedEntities.length > 0 && <p aria-label="Selected entity context">Selected: {selectedEntities.map(entity => `${entity.name} (#${entity.id})`).join(", ")}</p>}
    <label>Provider
      <select className="eui-select" value={provider} disabled={busy || loadingHistory || !providers} onChange={(event) => {
        setProvider(event.target.value); setConversationId(undefined); setEntries([]); setTruncated(false);
      }}>
        {!provider && <option value="">Choose an installed provider</option>}
        {providers?.providers.map((item) => <option key={item.id} value={item.id} disabled={!item.available}>{item.label}{item.available ? "" : " (not installed)"}</option>)}
      </select>
    </label>
    <p>{!providers ? "Checking providers and scene tools\u2026" : providers.sceneTools.available && providers.sceneTools.paired
      ? "Connected to scene tools and this SDK project."
      : providers.sceneTools.available ? "Project files are available. Pair the editor tab with the scene tools server to edit the live scene."
      : "Project files are available. Scene tools are not connected."}</p>
    {providers?.sceneTools.bridge && onPair && !providers.sceneTools.paired && <button type="button" className="eui-btn" disabled={busy || pairing || loadingHistory} onClick={() => {
      setPairing(true); setError(null);
      void onPair(providers.sceneTools.bridge!).then(() => assistant.providers()).then(setProviders)
        .catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
        .finally(() => setPairing(false));
    }}>{pairing ? "Pairing editor\u2026" : "Pair this editor"}</button>}
    {providers && !providers.providers.some((item) => item.available) && <p role="status">Install and sign in to a supported provider on the machine running the SDK, then reopen this panel.</p>}
    <div role="log" aria-label="Assistant conversation" className="eui-assistant__conversation">
      {entries.map((entry, index) => <div key={index} className={`eui-assistant__entry eui-assistant__entry--${entry.role}`}>
        <strong>{entry.role === "user" ? "You" : entry.role === "tool" ? "Tool" : "Assistant"}</strong>
        <p>{entry.text}</p>
      </div>)}
    </div>
    {error && <p role="alert">{error}</p>}
    <form onSubmit={(event) => { event.preventDefault(); void send(); }}>
      <label htmlFor="scene-assistant-prompt">Describe the scene change</label>
      <textarea id="scene-assistant-prompt" className="eui-input" rows={4} value={prompt} onChange={(event) => setPrompt(event.target.value)} disabled={busy} />
      <div className="eui-assistant__actions">
        <button type="button" className="eui-btn" disabled={busy || loadingHistory || !entries.length} onClick={() => { setConversationId(undefined); setEntries([]); setTruncated(false); }}>New conversation</button>
        {busy ? <button type="button" className="eui-btn" onClick={() => {
          const current = turnId.current;
          if (current) void assistant.cancel(current).catch((error: unknown) => setError(String(error)));
          request.current?.abort();
        }}>Stop</button> : <button type="submit" className="eui-btn primary" disabled={pairing || loadingHistory || !provider || !prompt.trim()}>Send</button>}
      </div>
    </form>
  </section>;
}
