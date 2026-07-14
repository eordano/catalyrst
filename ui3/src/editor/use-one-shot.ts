import { useEffect, useRef } from "react";

// One-shot command channel: the host bumps a monotonic nonce to say "apply this
// request NOW" -- the same request twice in a row (the user undid it in between)
// is invisible to a value-equality effect, which is why a plain prop cannot
// carry it. The callback runs only on the bump; its other inputs are read fresh
// through a ref, so call sites need no exhaustive-deps exception.
export function useOneShot(nonce: number, apply: () => void): void {
  const fn = useRef(apply);
  fn.current = apply;
  useEffect(() => {
    if (nonce <= 0) return;
    fn.current();
  }, [nonce]);
}
