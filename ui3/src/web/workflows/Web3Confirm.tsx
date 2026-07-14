import AuthLayout from "../frames/AuthLayout";
import "./web3confirm.css";

type Web3ConfirmProps = {
  code?: string;
  expiry?: string;
  onBack?: () => void;
  onExit?: () => void;
};

export default function Web3Confirm({
  code = "123456",
  expiry = "05:00",
  onBack,
  onExit,
}: Web3ConfirmProps) {
  const back = (
    <button className="web3__back" type="button" onClick={onBack}>
      <span className="web3__back-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none">
          <path
            d="m14 7-5 5 5 5"
            stroke="currentColor"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
      BACK
    </button>
  );

  const chrome = (
    <>
      <button className="web3__exit" type="button" onClick={onExit}>
        <span aria-hidden="true">&#x2190;]</span> EXIT
      </button>
      <button className="web3__icon-btn" type="button" aria-label="Mute audio">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
          <path
            d="M4 9v6h3l5 4V5L7 9H4Z"
            fill="currentColor"
          />
          <path
            d="M16 9c1 1 1 5 0 6M18.5 7c2 2 2 8 0 10"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
          />
        </svg>
      </button>
      <button className="web3__icon-btn" type="button" aria-label="Help">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
          <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" />
          <path
            d="M9.5 9.5a2.5 2.5 0 1 1 3.4 2.3c-.7.3-1 .8-1 1.5v.3"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <circle cx="11.9" cy="16.6" r="1" fill="currentColor" />
        </svg>
      </button>
      <span className="web3__version">editor-version - Editor</span>
    </>
  );

  return (
    <AuthLayout
      hideBrand
      hideFooter
      topLeft={back}
      bottomLeft={chrome}
      avatar={
        <div className="web3__art" aria-hidden="true">
          <div className="web3__lock">
            <div className="web3__shackle" />
            <div className="web3__body3d">
              <div className="web3__dial">
                <svg viewBox="0 0 48 48" className="web3__print">
                  <g
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2.2"
                    strokeLinecap="round"
                  >
                    <path d="M11 21c2-7 8-11 13-11s11 4 13 11" />
                    <path d="M14 26c0-7 5-11 10-11s10 4 10 11v3c0 6-3 10-8 11" />
                    <path d="M19 21c0-4 2-6 5-6s5 2 5 6v8c0 4-1 7-4 9" />
                    <path d="M24 19c2 0 3 2 3 5v6c0 3-1 6-3 8" />
                  </g>
                </svg>
              </div>
            </div>
          </div>
        </div>
      }
    >
      <div className="web3">
        <span className="web3__marker" role="img" aria-label="Decentraland">
          <svg viewBox="0 0 24 24" className="web3__marker-glyph" aria-hidden="true">
            <path d="M5 18.5 12 6l7 12.5H5Z" fill="rgba(255,255,255,.92)" />
            <path d="M9.4 18.5 14 10l4.6 8.5H9.4Z" fill="rgba(180,90,10,.55)" />
            <circle cx="12" cy="14.4" r="2" fill="#fff" />
          </svg>
        </span>

        <h1 className="web3__title">Secure Sign-in</h1>
        <p className="web3__instr">
          A browser window should open for you to sign into Decentraland. Check
          this number when prompted.
        </p>

        <div className="web3__code">{code}</div>

        <div className="web3__expire">
          <span>
            Verification number will expire in <b>{expiry}</b>
          </span>
          <svg viewBox="0 0 24 24" className="web3__clock" aria-hidden="true">
            <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.8" />
            <path d="M12 7v5l3 2" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </div>
      </div>
    </AuthLayout>
  );
}
