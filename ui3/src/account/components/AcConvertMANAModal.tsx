import type { ReactNode } from "react";
import { useState } from "react";
import AccountChrome from "../frames/AccountChrome";
import ManaMark from "../../atoms/ManaMark";
import Modal from "../../components/Modal";
import "./acconvertmanamodal.css";

type ConvertNetwork = "ethereum" | "matic";
type ConvertStage = "form" | "form-error" | "cost" | "cost-loading" | "auth";

type NetworkCopy = { title: string; subtitle: string; button: string };

const COPY = {
  ethereum: {
    title: "Convert to Polygon MANA",
    subtitle: "Deposit MANA from Ethereum into Polygon",
    button: "Complete Deposit",
  },
  matic: {
    title: "Convert to Ethereum MANA",
    subtitle: "Withdraw MANA from Polygon into Ethereum",
    button: "Start Withdrawal",
  },
} satisfies Record<ConvertNetwork, NetworkCopy>;

const FEES_WARNING =
  "Remember, any transaction that moves assets within, to, or from the Ethereum " +
  "blockchain will incur gas fees. Only transactions conducted exclusively on " +
  "Polygon are gas-less.";

function withdrawalCostCopy(cost: string): ReactNode {
  return (
    <>
      This operation consists of two steps. The first step won't have any cost.
      The second one will cost approximately <b>{cost}</b> ETH of gas fees.
    </>
  );
}

type AmountFormProps = {
  network: ConvertNetwork;
  amount: number;
  balance: number;
  manaPrice: number;
  onAmount: (raw: string) => void;
  onMax: () => void;
};

function AmountForm({ network, amount, balance, manaPrice, onAmount, onMax }: AmountFormProps) {
  const overBalance = balance < amount;
  const button = COPY[network].button;
  const disabled = overBalance || amount <= 0;
  return (
    <>
      <div className="acconvertmanamodal__field">
        <label className="acconvertmanamodal__fieldlabel" htmlFor="acconvert-amount">
          Amount
        </label>
        <div className="acconvertmanamodal__inputrow">
          <input
            id="acconvert-amount"
            className="acconvertmanamodal__input"
            inputMode="numeric"
            placeholder="0"
            value={amount === 0 ? "" : String(amount)}
            onChange={(e) => onAmount(e.target.value)}
          />
          <button
            type="button"
            className="acconvertmanamodal__maxbtn"
            onClick={onMax}
          >
            Max
          </button>
        </div>
      </div>

      {overBalance ? (
        <div className="acconvertmanamodal__amounterror">
          You don't have enough balance
        </div>
      ) : (
        <div className="acconvertmanamodal__usd">
          {(amount * manaPrice).toFixed(2)} USD
        </div>
      )}

      <div className="acconvertmanamodal__feeswarning">{FEES_WARNING}</div>

      <button
        type="button"
        className={
          "acconvertmanamodal__cta" +
          " acconvertmanamodal__cta--" +
          network +
          (disabled ? " is-disabled" : "")
        }
        disabled={disabled}
      >
        {button}
      </button>
    </>
  );
}

type AuthorizationStepProps = { amount: number; onBack: () => void };

function AuthorizationStep({ amount, onBack }: AuthorizationStepProps) {
  return (
    <div className="acconvertmanamodal__auth">
      <div className="acconvertmanamodal__authsteps">
        <span className="acconvertmanamodal__step is-active">
          <span className="acconvertmanamodal__stepdot">1</span>
          Authorize MANA
        </span>
        <span className="acconvertmanamodal__steprule" />
        <span className="acconvertmanamodal__step">
          <span className="acconvertmanamodal__stepdot">2</span>
          Confirm transaction
        </span>
      </div>

      <div className="acconvertmanamodal__authcard">
        <label className="acconvertmanamodal__authcaplabel" htmlFor="acconvert-cap">
          MANA Approved
        </label>
        <p className="acconvertmanamodal__authdesc">
          Enter what you want to deposit {amount} MANA or a higher amount you're
          comfortable with. You can change the limit at any time.
        </p>
        <div className="acconvertmanamodal__capfield">
          <input
            id="acconvert-cap"
            className="acconvertmanamodal__capinput"
            value={amount}
            readOnly
          />
          <span className="acconvertmanamodal__capunit">MANA</span>
        </div>
      </div>

      <button type="button" className="acconvertmanamodal__cta acconvertmanamodal__cta--ethereum">
        Authorize
      </button>
      <button
        type="button"
        className="acconvertmanamodal__authback"
        onClick={onBack}
      >
        Back
      </button>
    </div>
  );
}

type AcConvertMANAModalProps = {
  stage?: ConvertStage;
  network?: ConvertNetwork;
  manaEth?: number;
  manaMatic?: number;
  manaPrice?: number;
  cost?: string;
};

export default function AcConvertMANAModal({
  stage: initialStage = "form",
  network = "ethereum",
  manaEth = 2480.55,
  manaMatic = 1320.0,
  manaPrice = 0.41,
  cost = "0.0021",
}: AcConvertMANAModalProps) {
  const [stage, setStage] = useState<ConvertStage>(initialStage);
  const [amount, setAmount] = useState(stage.startsWith("form") ? 120 : 0);
  const [open, setOpen] = useState(true);

  const balance = network === "matic" ? manaMatic : manaEth;
  const copy = COPY[network];

  const effectiveAmount =
    stage === "form-error" ? Math.ceil(balance) + 500 : amount;

  function handleAmount(raw: string) {
    if (raw.length === 0) return setAmount(0);
    const n = parseInt(raw, 10);
    if (!isNaN(n)) setAmount(n);
  }

  const body =
    stage === "cost" || stage === "cost-loading" ? (
      <div className="acconvertmanamodal__feeswarning acconvertmanamodal__costgate">
        {stage === "cost-loading" ? (
          <span className="acconvertmanamodal__loader" role="status" aria-label="Loading cost" />
        ) : (
          withdrawalCostCopy(cost)
        )}
      </div>
    ) : stage === "auth" ? (
      <AuthorizationStep
        amount={effectiveAmount}
        onBack={() => setStage("form")}
      />
    ) : (
      <AmountForm
        network={network}
        amount={effectiveAmount}
        balance={balance}
        manaPrice={manaPrice}
        onAmount={handleAmount}
        onMax={() => setAmount(Math.floor(balance))}
      />
    );

  return (
    <div className="acconvertmanamodal">
      <AccountChrome
        mana={manaEth.toLocaleString(undefined, { maximumFractionDigits: 2 })}
        account=""
      >
        <div className="acconvertmanamodal__shellfill">
          <p className="acconvertmanamodal__shellhint">
            Convert MANA modal {"\u{2014}"} overlaid on the Wallets dashboard.
          </p>
        </div>
      </AccountChrome>

      {open && (
        <Modal
          onClose={() => setOpen(false)}
          width={520}
          ariaLabel={copy.title}
          className="acconvertmanamodal__card"
        >
          <div className="acconvertmanamodal__header">
            <div className="acconvertmanamodal__title">
              <span className="acconvertmanamodal__titlemark">
                <ManaMark size={18} />
              </span>
              {copy.title}
            </div>
            <div className="acconvertmanamodal__subtitle">{copy.subtitle}</div>
          </div>

          <div className="acconvertmanamodal__content">{body}</div>

          {stage === "cost" || stage === "cost-loading" ? (
            <div className="acconvertmanamodal__actions">
              <button
                type="button"
                className="acconvertmanamodal__cta acconvertmanamodal__cta--secondary"
              >
                Cancel
              </button>
              <button
                type="button"
                className="acconvertmanamodal__cta acconvertmanamodal__cta--matic"
                onClick={() => setStage("form")}
              >
                Proceed
              </button>
            </div>
          ) : null}
        </Modal>
      )}
    </div>
  );
}
