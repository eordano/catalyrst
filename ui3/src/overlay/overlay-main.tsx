import { StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";

import { queryClient } from "../app/queryClient";
import { isEditorShell, isNativeHost, startNativeHostBridge } from "./nativeHost";

if (typeof window !== "undefined") {
  window.dclDeferStart = true;
}

import "../atoms/primitives.css";
import "../styles.css";
import "../explorepanel.css";
import "../touch-targets.css";
import "../scene-backdrop.css";

if (import.meta.env.PROD) {
  const entryUrl = import.meta.url;
  globalThis.__UI3_ASSET_BASE__ = entryUrl.slice(0, entryUrl.lastIndexOf("/") + 1);
}

const isEditor = typeof window !== "undefined" && isEditorShell(window.location.search);

function mount(): void {
  void Promise.all([import("../app/BootGate"), import("../app/AppShell")]).then(
    ([{ default: BootGate }, { default: AppShell }]) => {
      let host = document.getElementById("ui3-overlay");
      if (!host) {
        host = document.createElement("div");
        host.id = "ui3-overlay";
        document.body.appendChild(host);
      }
      createRoot(host).render(
        <StrictMode>
          <QueryClientProvider client={queryClient}>
            <BootGate>
              <Suspense fallback={null}>
                <AppShell />
              </Suspense>
            </BootGate>
          </QueryClientProvider>
        </StrictMode>,
      );
      if (isNativeHost()) {
        startNativeHostBridge();
      }
    },
  );
}

if (isEditor) {
  window.dclDeferStart = false;
  const startNow = () => {
    void window.dclEngineStart?.();
  };
  if (window.dclEngineReady) startNow();
  else window.addEventListener("dcl-engine-ready", startNow, { once: true });
} else if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", mount, { once: true });
} else {
  mount();
}
