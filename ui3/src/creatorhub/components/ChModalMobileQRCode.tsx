import { useEffect, useId, useState } from "react";
import Modal from "../../components/Modal";
import "./chmodalmobileqrcode.css";

const COPY = {
  title: "Connect Mobile Debug Session",
  description: "Scan this QR code with your mobile device to preview the scene",
  disclaimer: "Both your computer and mobile device must be on the same network",
  waiting: "Waiting for mobile connection...",
  noAddress: "No preview address to share yet.",
};


export type MobileDebugSession = { id: number; messageCount: number };

type ChModalMobileQRCodeProps = {
  open?: boolean;
  url?: string;
  sessions?: MobileDebugSession[];
  simulateLive?: boolean;
  /** Forwarded to `Modal`. `false` renders the dialog in place instead of portalling it. */
  portal?: boolean;
  onClose?: () => void;
};

export default function ChModalMobileQRCode({
  open = true,
  url,
  sessions = [],
  simulateLive = false,
  portal = true,
  onClose = () => {},
}: ChModalMobileQRCodeProps) {
  const titleId = useId();
  const [qr, setQr] = useState<string | null>(null);

  // A real encoder, the same one data/auth/pair.ts uses. This modal previously
  // drew an xorshift matrix with fake finder patterns: it looked like a QR,
  // scanned as nothing, and sat under copy telling the user to scan it.
  useEffect(() => {
    if (!open || !url) {
      setQr(null);
      return undefined;
    }
    let live = true;
    void (async () => {
      const { default: qrcode } = await import("qrcode-generator");
      const code = qrcode(0, "M");
      code.addData(url);
      code.make();
      const svg = code.createSvgTag({ cellSize: 6, margin: 2, scalable: true });
      if (live) setQr(`data:image/svg+xml;utf8,${encodeURIComponent(svg)}`);
    })();
    return () => {
      live = false;
    };
  }, [open, url]);
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
          {qr !== null ? (
            <img src={qr} alt={`QR code for ${url ?? ""}`} className="chqr__qrimage" />
          ) : (
            <span className="chqr__url">{COPY.noAddress}</span>
          )}
        </div>

        <span className="chqr__url">{url ?? ""}</span>

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
