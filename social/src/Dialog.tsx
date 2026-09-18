import { useEffect, useRef, type ReactNode } from 'react';
import { Icon } from './Icon';
export function Dialog({ title, children, onClose, className = '' }: { title: string; children: ReactNode; onClose: () => void; className?: string }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const previous = document.activeElement as HTMLElement; ref.current?.showModal(); return () => previous?.focus(); }, []);
  return <dialog ref={ref} className={`social-dialog ${className}`} aria-label={title} onCancel={onClose} onClick={e => { if (e.target === e.currentTarget) { const r = e.currentTarget.getBoundingClientRect(); if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) onClose(); } }}><button className="dialog-close" aria-label="Close dialog" onClick={onClose}><Icon name="close" /></button>{children}</dialog>;
}
