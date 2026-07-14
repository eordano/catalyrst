import { useSyncExternalStore } from "react";

let open = false;
let pendingRedirect: string | null = null;
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

export function openSignIn(opts?: { redirectTo?: string }): void {
  pendingRedirect = opts?.redirectTo ?? null;
  if (!open) {
    open = true;
    emit();
  }
}

export function takePendingRedirect(): string | null {
  const r = pendingRedirect;
  pendingRedirect = null;
  return r;
}

export function closeSignIn(): void {
  if (open) {
    open = false;
    emit();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): boolean {
  return open;
}

function getServerSnapshot(): boolean {
  return false;
}

export function useSignInOpen(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
