import { useCallback, useEffect, useState } from "react";

import type { SettingEntry } from "../generated/bridge/SettingEntry";
import { attachBridge, sendBridge } from "./bridge";

export type EngineVariant = { label: string; description: string | null };

export type EngineSetting = {
  name: string;
  category: string;
  minValue: number;
  maxValue: number;
  stepSize: number;
  value: number;
  default: number;
  description: string | null;
  namedVariants: EngineVariant[];
};

export type EngineSettingInfo = Record<string, EngineSetting>;

const ATTACH_GIVE_UP_MS = 10000;

function isSettingsPush(push: unknown): push is { settings: SettingEntry[] } {
  const p = push as { kind?: unknown; settings?: unknown } | null;
  return !!p && p.kind === "settings" && Array.isArray(p.settings);
}

function asDescription(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

// Variants arrive as bare names from older engine builds and as
// {name, description} from builds that author tooltips; normalize both so
// consumers never branch on the wire shape.
function toVariant(v: unknown): EngineVariant | null {
  if (typeof v === "string") return { label: v, description: null };
  if (v && typeof v === "object") {
    const o = v as { name?: unknown; description?: unknown };
    if (typeof o.name === "string")
      return { label: o.name, description: asDescription(o.description) };
  }
  return null;
}

function toEngineSetting(s: SettingEntry): EngineSetting {
  const raw = s as SettingEntry & { description?: unknown };
  return {
    name: s.name,
    category: s.category,
    minValue: s.minValue,
    maxValue: s.maxValue,
    stepSize: s.stepSize,
    value: s.value,
    default: s.default,
    description: asDescription(raw.description),
    namedVariants: (s.namedVariants ?? [])
      .map(toVariant)
      .filter((v): v is EngineVariant => v !== null),
  };
}

// One GetSettings per mounted surface: the engine answers with a full snapshot
// push and echoes another after every SetSetting, so consumers render the
// clamped/applied values without a poll loop. Writes are optimistic; the echo
// reconciles them. `connected` goes false only after the attach loop gives up,
// so panels can say honestly that changes will not stick.
export function useEngineSettings(): {
  info: EngineSettingInfo | null;
  values: Record<string, number>;
  setValue: (name: string, value: number) => void;
  connected: boolean | null;
} {
  const [info, setInfo] = useState<EngineSettingInfo | null>(null);
  const [values, setValues] = useState<Record<string, number>>({});
  const [connected, setConnected] = useState<boolean | null>(null);

  useEffect(() => {
    let attached = false;
    const detach = attachBridge(
      (push) => {
        if (!isSettingsPush(push)) return;
        const byName: EngineSettingInfo = {};
        const next: Record<string, number> = {};
        for (const s of push.settings) {
          const e = toEngineSetting(s);
          byName[e.name] = e;
          next[e.name] = e.value;
        }
        setInfo(byName);
        setValues((prev) => ({ ...prev, ...next }));
      },
      () => {
        attached = true;
        setConnected(true);
        sendBridge("GetSettings", {});
      },
    );
    const giveUp = setTimeout(() => {
      if (!attached) setConnected(false);
    }, ATTACH_GIVE_UP_MS);
    return () => {
      clearTimeout(giveUp);
      detach();
    };
  }, []);

  const setValue = useCallback((name: string, value: number) => {
    setValues((prev) => ({ ...prev, [name]: value }));
    sendBridge("SetSetting", { name, value });
  }, []);

  return { info, values, setValue, connected };
}
