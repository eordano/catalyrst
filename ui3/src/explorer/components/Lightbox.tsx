import { useRef } from "react";
import { useDialogKeys } from "../../components/useDialogKeys";
import "./lightbox.css";

type LightboxProps = {
  src?: string | null;
  alt?: string;
  onClose?: () => void;
};

export default function Lightbox({ src, alt = "Photo", onClose }: LightboxProps) {
  const cardRef = useRef<HTMLDivElement>(null);
  useDialogKeys(cardRef, onClose);

  if (!src) return null;
  return (
    <div className="lb" role="dialog" aria-modal="true" tabIndex={-1} ref={cardRef} onClick={onClose}>
      <button className="lb__close" aria-label="Close" onClick={onClose}>&#xD7;</button>
      <img
        className="lb__img"
        src={src}
        alt={alt}
        onClick={(e) => e.stopPropagation()}
      />
    </div>
  );
}
