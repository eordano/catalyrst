import { useSyncExternalStore } from "react";

export type ChatIntent = { kind: "direct"; id: string; nonce: number };

let current: ChatIntent | null = null;
let nonce = 0;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function requestDirectMessage(address: string) {
  if (!address) return;
  nonce += 1;
  current = { kind: "direct", id: address, nonce };
  emit();
}

export function consumeChatIntent(consumed: number) {
  if (current?.nonce !== consumed) return;
  current = null;
  emit();
}

export const useChatIntent = (): ChatIntent | null => useSyncExternalStore(subscribe, () => current, () => null);
