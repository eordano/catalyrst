import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { RemoteTrack, Room } from "livekit-client";

import type { NearbyPlayer } from "../../generated/bridge/NearbyPlayer";
import type { BridgeChatLine } from "../../overlay/bridge";
import { ChatView, type ChatIo } from "../../explorer/frames/Chat";
import "./fdroomdock.css";

// Every foundry page is a LiveKit room named by its path, and the room's chat
// IS the explorer's chat — same component, same look — docked bottom-right.
// Everything shown is the room's live state: who is connected, what they said
// while you were here, who is speaking. Nothing is recorded; leaving the page
// leaves the room and the messages go with it.

export type FdRoomTicket = {
  token: string;
  wsUrl: string;
  room: string;
  identity: string;
  name: string;
};

export type FdRoomDockProps = {
  /** The page path, which names the room; a change moves the dock to the new room. */
  path: string;
  /** POSTs the path to the server and resolves the join ticket, or null when
   *  rooms are not configured / the request failed — the dock then renders nothing. */
  getTicket: (path: string) => Promise<FdRoomTicket | null>;
  onEvent?: (event: "joined" | "message_sent" | "mic_on", others: number) => void;
};

const MAX_LINES = 200;

/** The room's channel name in the chat header: the page's own last segment. */
export function roomTitle(path: string): string {
  const seg = path.replace(/\/+$/, "").split("/").filter(Boolean).pop();
  return !seg || seg === "foundry" ? "the front door" : seg;
}

function toPlayers(room: Room): NearbyPlayer[] {
  return [...room.remoteParticipants.values()].map((p) => ({
    address: p.identity,
    name: p.name || p.identity.slice(0, 4),
    wearables: [],
    coords: "",
  }));
}

export default function FdRoomDock({ path, getTicket, onEvent }: FdRoomDockProps) {
  const [connected, setConnected] = useState(false);
  const [elsewhere, setElsewhere] = useState(0);
  const [players, setPlayers] = useState<NearbyPlayer[]>([]);
  const [lines, setLines] = useState<BridgeChatLine[]>([]);
  const [open, setOpen] = useState(false);
  const [micOn, setMicOn] = useState(false);
  const roomRef = useRef<Room | null>(null);
  const selfRef = useRef<{ identity: string; name: string } | null>(null);
  const audioRef = useRef<HTMLDivElement>(null);

  const append = useCallback((line: BridgeChatLine) => {
    setLines((prev) => [...prev.slice(-MAX_LINES + 1), line]);
  }, []);

  useEffect(() => {
    let dead = false;
    let room: Room | null = null;

    (async () => {
      const ticket = await getTicket(path);
      if (dead || !ticket) return;
      const lk = await import("livekit-client");
      if (dead) return;

      room = new lk.Room();
      roomRef.current = room;
      selfRef.current = { identity: ticket.identity, name: ticket.name };

      const refresh = () => {
        if (!dead && room) setPlayers(toPlayers(room));
      };
      room
        .on(lk.RoomEvent.ParticipantConnected, refresh)
        .on(lk.RoomEvent.ParticipantDisconnected, refresh)
        .on(lk.RoomEvent.DataReceived, (payload, participant) => {
          try {
            const msg = JSON.parse(new TextDecoder().decode(payload));
            if (msg?.t !== "chat" || typeof msg.text !== "string") return;
            append({
              senderName: participant?.name || participant?.identity?.slice(0, 4),
              senderAddress: participant?.identity,
              message: msg.text.slice(0, 500),
              channel: "page",
              timestamp: Date.now(),
            });
          } catch {
            // Not our payload shape — some other client's data, ignored.
          }
        })
        .on(lk.RoomEvent.TrackSubscribed, (track: RemoteTrack) => {
          if (track.kind === "audio" && audioRef.current) {
            audioRef.current.appendChild(track.attach());
          }
        })
        .on(lk.RoomEvent.TrackUnsubscribed, (track: RemoteTrack) => {
          track.detach().forEach((el) => el.remove());
        });

      try {
        await room.connect(ticket.wsUrl, ticket.token);
      } catch {
        return;
      }
      if (dead) {
        room.disconnect();
        return;
      }
      setConnected(true);
      refresh();
      onEvent?.("joined", room.remoteParticipants.size);
    })();

    return () => {
      dead = true;
      roomRef.current = null;
      selfRef.current = null;
      room?.disconnect();
      setConnected(false);
      setPlayers([]);
      setLines([]);
      setMicOn(false);
      setOpen(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  const others = players.length;

  const io = useMemo<ChatIo>(
    () => ({
      chat: lines,
      players,
      blocked: [],
      live: connected,
      me: selfRef.current
        ? { address: selfRef.current.identity, name: selfRef.current.name }
        : null,
      send: (message) => {
        const room = roomRef.current;
        const self = selfRef.current;
        if (!room || !self) return;
        room.localParticipant.publishData(
          new TextEncoder().encode(JSON.stringify({ t: "chat", text: message })),
          { reliable: true },
        );
        append({
          senderName: self.name,
          senderAddress: self.identity,
          message,
          channel: "page",
          timestamp: Date.now(),
        });
        onEvent?.("message_sent", roomRef.current?.remoteParticipants.size ?? 0);
      },
    }),
    [lines, players, connected, append, onEvent],
  );

  useEffect(() => {
    // Site-wide presence for the collapsed pill: who is in OTHER page rooms
    // right now. A failed poll leaves the last honest reading; zero renders
    // nothing extra rather than a claim.
    let dead = false;
    const mine = path.replace(/\/+$/, "") || "/foundry";
    const load = async () => {
      try {
        const res = await fetch("/foundry/room-token");
        const body = (await res.json()) as {
          presence?: { path: string; count: number }[] | null;
        };
        if (dead || !Array.isArray(body.presence)) return;
        setElsewhere(
          body.presence
            .filter((r) => r.path.replace(/\/+$/, "") !== mine)
            .reduce((a, r) => a + r.count, 0),
        );
      } catch {
        // Network hiccup: keep the previous reading.
      }
    };
    void load();
    const t = setInterval(load, 90_000);
    return () => {
      dead = true;
      clearInterval(t);
    };
  }, [path]);

  if (!connected) return null;

  async function toggleOpen() {
    const next = !open;
    setOpen(next);
    if (next) await roomRef.current?.startAudio().catch(() => undefined);
  }

  async function toggleMic() {
    const room = roomRef.current;
    if (!room) return;
    const next = !micOn;
    try {
      await room.localParticipant.setMicrophoneEnabled(next);
      setMicOn(next);
      if (next) onEvent?.("mic_on", others);
    } catch {
      setMicOn(false);
    }
  }

  return (
    <aside className={"fd-room" + (open ? " is-open" : "")} aria-label="Page room">
      <div ref={audioRef} hidden />
      {open ? (
        <div className="fd-room__chat">
          <ChatView
            open
            docked
            title={roomTitle(path)}
            membersTitle="In this room"
            membersEmpty="Nobody else is in this room."
            emptyLine="Nothing said since you arrived — room messages aren't recorded."
            io={io}
            onToggle={() => setOpen(false)}
          />
        </div>
      ) : null}
      <div className="fd-room__bar">
        {open ? (
          <button
            type="button"
            className={"fd-room__mic" + (micOn ? " is-on" : "")}
            onClick={toggleMic}
            aria-pressed={micOn}
            title={micOn ? "Mute microphone" : "Talk — unmute microphone"}
          >
            {micOn ? "mic on" : "mic"}
          </button>
        ) : null}
        <button
          type="button"
          className={"fd-room__pill" + (others > 0 ? " has-others" : "")}
          onClick={toggleOpen}
          aria-expanded={open}
        >
          {others > 0
            ? `${others} ${others === 1 ? "other" : "others"} here`
            : elsewhere > 0
              ? `just you — ${elsewhere} elsewhere`
              : "just you"}
        </button>
      </div>
    </aside>
  );
}
