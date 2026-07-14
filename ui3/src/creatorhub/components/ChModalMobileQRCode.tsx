import { useEffect, useId, useState } from "react";
import Modal from "../../components/Modal";
import "./chmodalmobileqrcode.css";

const COPY = {
  title: "Connect Mobile Debug Session",
  description: "Scan this QR code with your mobile device to preview the scene",
  disclaimer: "Both your computer and mobile device must be on the same network",
  waiting: "Waiting for mobile connection...",
};

function makeQrDataUri(seed = 0) {
  const N = 25;
  const M = 8;
  const dark = "#000000";
  let rng = 2166136261 ^ seed;
  const next = () => {
    rng ^= rng << 13;
    rng ^= rng >>> 17;
    rng ^= rng << 5;
    return ((rng >>> 0) % 1000) / 1000;
  };
  const inFinder = (x: number, y: number) => {
    const eye = (ox: number, oy: number) => {
      const dx = x - ox;
      const dy = y - oy;
      if (dx < 0 || dy < 0 || dx > 6 || dy > 6) return null;
      const border = dx === 0 || dy === 0 || dx === 6 || dy === 6;
      const core = dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4;
      return border || core;
    };
    const a = eye(0, 0);
    if (a !== null) return a;
    const b = eye(N - 7, 0);
    if (b !== null) return b;
    const c = eye(0, N - 7);
    if (c !== null) return c;
    return undefined;
  };
  const isQuiet = (x: number, y: number) =>
    (x <= 7 && y <= 7) ||
    (x >= N - 8 && y <= 7) ||
    (x <= 7 && y >= N - 8);
  let rects = "";
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const f = inFinder(x, y);
      let on;
      if (f !== undefined) {
        on = f;
      } else if (isQuiet(x, y)) {
        on = false;
      } else {
        on = next() > 0.52;
      }
      if (on) rects += `<rect x="${x * M}" y="${y * M}" width="${M}" height="${M}"/>`;
    }
  }
  const px = N * M;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 ${px} ${px}">` +
    `<rect width="${px}" height="${px}" fill="#ffffff"/>` +
    `<g fill="${dark}">${rects}</g>` +
    `</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

export type MobileDebugSession = { id: number; messageCount: number };

type ChModalMobileQRCodeProps = {
  open?: boolean;
  url?: string;
  qr?: string;
  sessions?: MobileDebugSession[];
  simulateLive?: boolean;
  /** Forwarded to `Modal`. `false` renders the dialog in place instead of portalling it. */
  portal?: boolean;
  onClose?: () => void;
};

export default function ChModalMobileQRCode({
  open = true,
  url = "http://192.168.1.42:8000/?realm=LocalPreview&position=0,0",
  qr = makeQrDataUri(7),
  sessions = [],
  simulateLive = false,
  portal = true,
  onClose = () => {},
}: ChModalMobileQRCodeProps) {
  const titleId = useId();
  const [liveSessions, setLiveSessions] = useState<MobileDebugSession[]>(sessions);

  useEffect(() => {
    setLiveSessions(sessions);
  }, [sessions]);

  useEffect(() => {
    if (!open) {
      setLiveSessions(sessions);
      return undefined;
    }
    if (!simulateLive || sessions.length > 0) return undefined;
    const t = setTimeout(() => {
      setLiveSessions([{ id: 1, messageCount: 1284 }]);
    }, 1600);
    return () => clearTimeout(t);
  }, [open, simulateLive]);

  if (!open) return null;

  return (
    <Modal
      width={540}
      className="chqr"
      ariaLabelledBy={titleId}
      onClose={onClose}
      closeOnBackdrop={false}
      portal={portal}
    >
      <h2 className="chqr__title" id={titleId}>{COPY.title}</h2>
      <p className="chqr__subtitle">{COPY.description}</p>

      <div className="chqr__content">
        <div className="chqr__qrcontainer">
          <img src={qr} alt="QR Code" className="chqr__qrimage" />
        </div>

        <span className="chqr__url">{url}</span>

        <div className="chqr__sessions">
          {liveSessions.length === 0 ? (
            <span className="chqr__status chqr__status--waiting">
              {COPY.waiting}
            </span>
          ) : (
            liveSessions.map((s) => (
              <div key={s.id} className="chqr__session">
                <span className="chqr__badge chqr__badge--connected">
                  Session #{s.id}
                </span>
                <span className="chqr__messages">
                  {s.messageCount.toLocaleString()} entries
                </span>
              </div>
            ))
          )}
        </div>

        <span className="chqr__disclaimer">{COPY.disclaimer}</span>
      </div>
    </Modal>
  );
}
