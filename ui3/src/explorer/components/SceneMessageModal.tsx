import { useEffect, useState } from "react";
import Button from "../../atoms/Button";
import Modal from "../../components/Modal";
import { connectWallet, walletProvider } from "../../data/auth/wallet";
import { feedbackRoute, REQUEST_CONFIRM_TIMEOUT_MS, sendOwnerMessage } from "../../data/catalyst/sceneFeedback";
import { sceneRecipients, type SceneRecipient } from "../../data/catalyst/sceneOwner";
import { flagState } from "../../data/featureFlags";
import { FRIEND_ACTIONS, requestFriendAction, useFriends } from "../../data/hooks/useFriends";
import { useBridgeState } from "../../overlay/bridge";
import SceneRecipientChoices from "./SceneRecipientChoices";
import { chainName, fetchTipTarget, manaToWei, sendManaTip, type TipTarget } from "./manaTip";
import { composeFeedback, feedbackHeader, FEEDBACK_MAX } from "./sceneMessage";
import "./sceneowneractions.css";

type Props = {
  owner?: string | null;
  recipients?: SceneRecipient[];
  loading?: boolean;
  sceneTitle: string;
  coords: string;
  onClose: () => void;
};
type Phase = "compose" | "wallet" | "sending" | "confirming" | "messaged" | "requested" | "unconfirmed" | "tipped";

export function SceneFeedbackModal(props: Props) { return <SceneMessageModal {...props} intent="feedback" />; }
export function SceneTipModal(props: Props) { return <SceneMessageModal {...props} intent="tip" />; }

function SceneMessageModal({ owner: initialOwner, recipients, loading = false, sceneTitle, coords, onClose, intent }: Props & { intent: "feedback" | "tip" }) {
  const [combined] = useState(() => flagState("2026-09-scene-feedback-tip").enabled);
  const [includeTip, setIncludeTip] = useState(intent === "tip");
  const [selected, setSelected] = useState<string>();
  const choices = recipients ?? sceneRecipients(initialOwner ? { entityId: "", title: sceneTitle, deployer: initialOwner } : null, null);
  const recipient = choices.find(item => item.role === selected && item.address)
    ?? (intent === "tip" ? choices.find(item => item.address === initialOwner) : undefined)
    ?? choices.find(item => item.role === "sceneAuthor" && item.address)
    ?? choices.find(item => item.role === "sceneDeployer" && item.address)
    ?? choices.find(item => item.address);
  const owner = recipient?.address ?? "";
  const identity = useBridgeState(s => s.identity);
  const { friends, sent, received, isPending: friendsPending } = useFriends();
  const route = feedbackRoute(owner, friends, sent, received);
  const self = !!owner && (identity.address ?? "").toLowerCase() === owner.toLowerCase();
  const [text, setText] = useState("");
  const [amount, setAmount] = useState("5");
  const [target, setTarget] = useState<TipTarget | null>(null);
  const [targetError, setTargetError] = useState("");
  const [phase, setPhase] = useState<Phase>("compose");
  const [error, setError] = useState("");
  const [hash, setHash] = useState("");
  const [copied, setCopied] = useState(false);
  const provider = includeTip ? walletProvider() : null;
  const wei = manaToWei(amount);
  const remaining = Math.max(0, FEEDBACK_MAX - feedbackHeader(sceneTitle, coords).length);
  const body = text.trim();
  const busy = ["wallet", "sending", "confirming"].includes(phase);
  const done = ["messaged", "requested", "unconfirmed", "tipped"].includes(phase);
  const noteAllowed = !identity.isGuest && !self && (route === "message" || route === "request");
  const asFriendRequest = noteAllowed && route === "request" && !loading && !!owner && !friendsPending && (!includeTip || !!body);
  const canSend = !loading && !!owner && !self && !busy && !done
    && (body ? noteAllowed && body.length <= remaining : includeTip && !hash)
    && (!includeTip || !!hash || (!!provider && !!target && !!wei));
  const title = combined ? "Send feedback" : intent === "tip" ? "Send a MANA tip" : "Send feedback";

  useEffect(() => {
    if (!includeTip || target) return;
    const ac = new AbortController();
    setTargetError("");
    fetchTipTarget(ac.signal).then(value => {
      if (ac.signal.aborted) return;
      if (value) setTarget(value);
      else setTargetError("This server has no MANA payment configuration.");
    }).catch(() => { if (!ac.signal.aborted) setTargetError("Could not load the MANA payment configuration."); });
    return () => ac.abort();
  }, [includeTip, target]);

  const requestSeen = route === "pending" || route === "message";
  useEffect(() => {
    if (phase !== "confirming") return;
    if (requestSeen) { setPhase("requested"); return; }
    const timer = setTimeout(() => setPhase("unconfirmed"), REQUEST_CONFIRM_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [phase, requestSeen]);

  async function send() {
    if (!canSend) return;
    setError("");
    try {
      if (includeTip && !hash && provider && target && wei) {
        setPhase("wallet");
        const from = await connectWallet();
        const tx = await sendManaTip(provider, { from, to: owner, wei, target });
        setHash(tx);
      }
      if (!body) { setPhase("tipped"); return; }
      const note = composeFeedback(sceneTitle, coords, text);
      if (route === "message") {
        setPhase("sending");
        await sendOwnerMessage(owner, note);
        setPhase("messaged");
      } else {
        const handed = requestFriendAction(FRIEND_ACTIONS.REQUEST, owner, { message: note });
        if (!handed) throw new Error("The friend request could not be handed to the client.");
        setPhase("confirming");
      }
    } catch (e) {
      setError(e instanceof Error && e.message ? e.message : "Could not send. Please try again.");
      setPhase("compose");
    }
  }

  async function copyOwner() {
    try { await navigator.clipboard.writeText(owner); setCopied(true); }
    catch { setError("Could not copy the address. Select and copy it below."); }
  }

  return <Modal onClose={onClose} ariaLabel={intent === "tip" && !combined ? "Send tip" : title} className="soa" width={520}>
    <h2 className="soa__title">{title}</h2>
    <p className="soa__meta"><span className="soa__scene">{sceneTitle || "This scene"}</span> &middot; {coords}</p>
    <SceneRecipientChoices recipients={choices} selected={recipient?.role} disabled={busy || done || !!hash} loading={loading} onSelect={role => { setSelected(role); setError(""); setCopied(false); }} />
    {hash && <div role="status"><p className="soa__ok">Tip submitted on {target ? chainName(target.chainId) : "chain"}. Your wallet can confirm the transaction.</p><div className="soa__hash">{hash}</div></div>}
    {done ? <>
      {phase === "messaged" && <p className="soa__ok">Delivered. Your note reached the recipient as a direct message.</p>}
      {phase === "requested" && <p className="soa__ok">Your friend request is on its way; the recipient will find your note under Friends &rsaquo; Requests.</p>}
      {phase === "unconfirmed" && <p className="soa__warn">The client accepted the request but did not confirm it went out. Check Friends &rsaquo; Requests before sending it again.</p>}
      <div className="soa__actions"><Button onClick={onClose}>Done</Button></div>
    </> : <>
      <label className="soa__message-label" htmlFor="scene-feedback-message">{includeTip ? "Message (optional)" : "Feedback"}</label>
      <textarea id="scene-feedback-message" className="soa__text" value={text} onChange={e => setText(e.target.value)} maxLength={remaining} rows={3} placeholder="What did you enjoy? What would you love to see next?" aria-label="Feedback" disabled={busy} autoFocus />
      <div className="soa__hint">{body.length}/{remaining}</div>
      {combined && <label className="soa__tip-toggle"><input type="checkbox" checked={includeTip} disabled={busy || !!hash} onChange={e => setIncludeTip(e.target.checked)} />Add a MANA tip</label>}
      {includeTip && !hash && <fieldset className="soa__payment" disabled={busy}>
        <legend>Tip amount</legend>
        <div className="soa__amounts" role="group" aria-label="Quick amounts">{["1", "5", "10", "50"].map(a => <button key={a} type="button" className="soa__chip" aria-pressed={amount === a} onClick={() => setAmount(a)}>{a} MANA</button>)}</div>
        <label className="soa__amount"><span>Amount</span><input className="soa__input" inputMode="decimal" value={amount} onChange={e => setAmount(e.target.value)} aria-label="Tip amount in MANA" /><span>MANA</span></label>
        {amount.trim() !== "" && !wei && <p className="soa__warn">Enter a positive MANA amount.</p>}
        {targetError && <p className="soa__warn">{targetError}</p>}
        {!target && !targetError && <p role="status" className="soa__hint">Loading payment options&hellip;</p>}
        {!provider && <div className="soa__fallback"><p className="soa__warn">No browser wallet found here. Send MANA{target ? ` on ${chainName(target.chainId)}` : ""} to this recipient from any wallet:</p><div className="soa__addr"><span>{owner}</span><Button size="sm" variant="secondary" onClick={copyOwner} disabled={!owner}>{copied ? "Copied" : "Copy address"}</Button></div></div>}
        {target && provider && <p className="soa__hint">Your wallet will ask you to confirm {wei ? amount : "the amount of"} MANA on {chainName(target.chainId)}.</p>}
      </fieldset>}
      {identity.isGuest && (!includeTip || !!body) && <p className="soa__warn">Sign in with a wallet to send feedback.</p>}
      {self && <p className="soa__warn">This is your wallet. Choose another recipient.</p>}
      {asFriendRequest && <p className="soa__note" role="note">You and this recipient are not friends yet, so your feedback will be sent as a friend request. They will find your note under Friends &rsaquo; Requests.</p>}
      {body && route === "pending" && <p className="soa__warn">Your friend request is still pending. Send a message once they accept it.</p>}
      {body && route === "incoming" && <p className="soa__warn">Accept this recipient&rsquo;s request under Friends &rsaquo; Requests before sending a message.</p>}
      {error && <p className="soa__warn soa__err" role="alert">{error}{hash ? " Your tip was already submitted; retrying will only send the message." : ""}</p>}
      <div className="soa__actions"><Button variant="secondary" onClick={onClose}>Cancel</Button><Button onClick={send} disabled={!canSend}>{phase === "wallet" ? "Check your wallet\u2026" : phase === "sending" ? "Sending\u2026" : phase === "confirming" ? "Confirming\u2026" : hash ? "Retry message" : includeTip ? `Send ${wei ? amount.trim() : ""} MANA${body ? " and message" : ""}` : route === "message" ? "Send message" : "Send feedback"}</Button></div>
    </>}
  </Modal>;
}
