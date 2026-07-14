import type { CSSProperties } from "react";
import { useState } from "react";
import Toggle from "../../atoms/Toggle";
import { sendBridge } from "../../overlay/bridge";
import "./skybox.css";

const fmt = (min: number) => {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
};

export default function SkyboxHUD() {
  const [minutes, setMinutes] = useState(990);
  const [auto, setAuto] = useState(true);

  function setTime(min: number, isAuto: boolean) {
    sendBridge("SetTimeOfDay", { minutes: min, auto: isAuto });
  }
  function onAuto(v: boolean) {
    setAuto(v);
    setTime(minutes, v);
  }
  function onSlide(min: number) {
    setMinutes(min);
    setTime(min, false);
  }

  const rangeStyle: CSSProperties & { "--pct": string } = {
    "--pct": (minutes / 1439) * 100 + "%",
  };

  return (
    <div className="sky__backdrop">
      <div className="sky">
        <div className="sky__head">
          <span className="sky__title">NIGHT/DAY</span>
        </div>

        <div className="sky__row sky__row--auto">
          <span className="sky__label">Auto</span>
          <Toggle checked={auto} onChange={onAuto} ariaLabel="Auto" />
        </div>

        <div className={"sky__group" + (auto ? " is-dim" : "")}>
          <div className="sky__row">
            <label className="sky__label sky__label--muted" htmlFor="sky-time-range">Custom</label>
            <span className="sky__time">{fmt(minutes)}</span>
          </div>

          <div className="sky__slider">
            <div className="sky__track">
              <input
                type="range" className="sky__range" id="sky-time-range"
                min="0" max="1439" step="1" value={minutes}
                disabled={auto}
                onChange={(e) => onSlide(Number(e.target.value))}
                style={rangeStyle}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
