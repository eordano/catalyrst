import { useEffect, useRef, useState } from "react";
import { useOrientation } from "./OrientationProvider";
import "./viewport.css";
import "./layout.css";

export type SafeAreaDebugOverlayProps = {
  enabled?: boolean;
  className?: string;
};

type SafeAreaMetrics = {
  width: number;
  height: number;
  uiPx: number;
  top: number;
  right: number;
  bottom: number;
  left: number;
};

const PROBE_SPAN = 1000;

function round(value: number, digits: number): number {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

export default function SafeAreaDebugOverlay({
  enabled = true,
  className,
}: SafeAreaDebugOverlayProps) {
  const orientation = useOrientation();
  const probeRef = useRef<HTMLDivElement>(null);
  const topRef = useRef<HTMLDivElement>(null);
  const rightRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const leftRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState<SafeAreaMetrics | null>(null);

  useEffect(() => {
    if (!enabled || typeof window === "undefined") return undefined;
    const measure = () => {
      const probe = probeRef.current;
      setMetrics({
        width: Math.round(window.innerWidth),
        height: Math.round(window.innerHeight),
        uiPx: probe ? probe.getBoundingClientRect().width / PROBE_SPAN : 0,
        top: topRef.current ? topRef.current.getBoundingClientRect().height : 0,
        right: rightRef.current ? rightRef.current.getBoundingClientRect().width : 0,
        bottom: bottomRef.current ? bottomRef.current.getBoundingClientRect().height : 0,
        left: leftRef.current ? leftRef.current.getBoundingClientRect().width : 0,
      });
    };
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("orientationchange", measure);
    const view = window.visualViewport;
    if (view) view.addEventListener("resize", measure);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("orientationchange", measure);
      if (view) view.removeEventListener("resize", measure);
    };
  }, [enabled, orientation]);

  if (!enabled) return null;

  const insets = metrics
    ? `${round(metrics.top, 1)} / ${round(metrics.right, 1)} / ${round(metrics.bottom, 1)} / ${round(metrics.left, 1)}`
    : "\u{2014}";

  return (
    <div className={className ? `msd ${className}` : "msd"} aria-hidden="true">
      <div className="msd__probe" ref={probeRef} />
      <div className="msd__strip msd__strip--top" ref={topRef} />
      <div className="msd__strip msd__strip--right" ref={rightRef} />
      <div className="msd__strip msd__strip--bottom" ref={bottomRef} />
      <div className="msd__strip msd__strip--left" ref={leftRef} />
      <div className="msd__hud">
        <div className="msd__row">
          <span className="msd__key">orientation</span>
          <span className="msd__val">{orientation}</span>
        </div>
        <div className="msd__row">
          <span className="msd__key">viewport</span>
          <span className="msd__val">
            {metrics ? `${metrics.width} x ${metrics.height}` : "\u{2014}"}
          </span>
        </div>
        <div className="msd__row">
          <span className="msd__key">--ui-px</span>
          <span className="msd__val">{metrics ? round(metrics.uiPx, 4) : "\u{2014}"}</span>
        </div>
        <div className="msd__row">
          <span className="msd__key">design base</span>
          <span className="msd__val">
            {orientation === "portrait" ? "720 x 1600" : "1600 x 720"}
          </span>
        </div>
        <div className="msd__row">
          <span className="msd__key">insets t/r/b/l</span>
          <span className="msd__val">{insets}</span>
        </div>
      </div>
    </div>
  );
}
