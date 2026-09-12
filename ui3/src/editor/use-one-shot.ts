import { useEffect, useRef } from "react";

export function useOneShot(nonce: number, apply: () => void): void {
  const fn = useRef(apply);
  fn.current = apply;
  useEffect(() => {
    if (nonce <= 0) return;
    fn.current();
  }, [nonce]);
}
