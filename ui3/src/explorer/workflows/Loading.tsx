import { useEffect, useRef, useState } from "react";
import { LOADING_TIPS, TIP_ROTATION_MS } from "./loadingTips";
import "./loading.css";

const TIP_INDEX_STORAGE_KEY = "dcl-loading-tip-index";

const TIP_COUNT = LOADING_TIPS.length;

function DclGem() {
  return (
    <svg className="loading__gem" viewBox="0 0 32 32" aria-hidden="true">
      <circle cx="16" cy="16" r="16" fill="#ff2d55" />
      <path
        d="M16 7l6 6-6 6-6-6 6-6zm0 13.5l5.5-5.5v3L16 23.5 10.5 18v-3L16 20.5z"
        fill="#fff"
      />
    </svg>
  );
}

function persistedTip(): number {
  try {
    const n = Number(localStorage.getItem(TIP_INDEX_STORAGE_KEY));
    return Number.isInteger(n) && n >= 0 && n < TIP_COUNT ? n : 0;
  } catch {
    return 0;
  }
}

type LoadingProps = { progress?: number; initialTip?: number };

export default function Loading({ progress = 65, initialTip }: LoadingProps) {
  const phaseLabel = progress < 50 ? "Booting engine" : "Loading world";
  const [tip, setTip] = useState(() => initialTip ?? persistedTip());
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  const startTimer = () => {
    if (timer.current !== null) clearInterval(timer.current);
    timer.current = setInterval(() => {
      setTip((t) => (t + 1) % TIP_COUNT);
    }, TIP_ROTATION_MS);
  };

  useEffect(() => {
    startTimer();
    return () => {
      if (timer.current !== null) clearInterval(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(TIP_INDEX_STORAGE_KEY, String(tip));
    } catch {
    }
  }, [tip]);

  const goTo = (i: number) => {
    setTip(((i % TIP_COUNT) + TIP_COUNT) % TIP_COUNT);
    startTimer();
  };

  return (
    <div className="loading">
      <header className="loading__header">
        <div className="loading__brand">
          <DclGem />
          <span>Decentraland</span>
        </div>
        <div className="loading__pct">{phaseLabel} {progress}%</div>
      </header>
      <div className="loading__bar">
        <div className="loading__bar-fill" style={{ width: progress + "%" }} />
      </div>

      <div className="loading__stage">
        {LOADING_TIPS.map((t, i) => (
          <div
            key={t.title}
            className={"loading__pane" + (i === tip ? " is-active" : "")}
            aria-hidden={i !== tip}
          >
            <img
              className="loading__art"
              src={i === tip || i === (tip + 1) % TIP_COUNT ? t.art : undefined}
              loading={i === tip ? "eager" : "lazy"}
              decoding="async"
              alt=""
            />
            <div className="loading__text">
              <h1 className="loading__title">{t.title}</h1>
              <p className="loading__body">{t.body}</p>
            </div>
          </div>
        ))}
        <div className="loading__dots">
          {LOADING_TIPS.map((t, i) => (
            <button
              key={t.title}
              type="button"
              className={"loading__dot" + (i === tip ? " is-active" : "")}
              aria-label={"Tip " + (i + 1)}
              onClick={() => goTo(i)}
            />
          ))}
        </div>
      </div>

      <button
        type="button"
        className="loading__arrow loading__arrow--left"
        aria-label="Previous tip"
        onClick={() => goTo(tip - 1)}
      >
        ‹
      </button>
      <button
        type="button"
        className="loading__arrow loading__arrow--right"
        aria-label="Next tip"
        onClick={() => goTo(tip + 1)}
      >
        ›
      </button>
    </div>
  );
}
