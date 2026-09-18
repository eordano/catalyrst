import { useEffect, useState } from "react";
import { isNativeHost } from "../../overlay/nativeHost";
import "./graphicsprogress.css";

export default function GraphicsProgress() {
  const [pending, setPending] = useState(false);
  useEffect(() => {
    let waiting = false;
    let frames = 0;
    const change = () => {
      if (isNativeHost()) return;
      waiting = true;
      frames = 0;
      setPending(true);
    };
    const frame = () => {
      if (!waiting || ++frames < 2) return;
      waiting = false;
      setPending(false);
    };
    window.addEventListener("dcl-graphics-change", change);
    window.addEventListener("dcl-engine-frame", frame);
    return () => {
      window.removeEventListener("dcl-graphics-change", change);
      window.removeEventListener("dcl-engine-frame", frame);
    };
  }, []);
  return pending ? <div className="graphics-progress" role="status"><span>{"Applying graphics settings\u2026"}</span></div> : null;
}
