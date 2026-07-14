import type { ComponentPropsWithoutRef, ReactNode } from "react";
import "./assetactionlayout.css";

function TextBack({ label, onBack }: { label?: ReactNode; onBack?: () => void }) {
  return (
    <button type="button" className="assetaction__back" onClick={onBack}>
      <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M12 4l-6 6 6 6" />
      </svg>
      {label}
    </button>
  );
}

function IconBack({ onBack }: { onBack?: () => void }) {
  return (
    <button type="button" className="assetaction__backicon" aria-label="Back" onClick={onBack}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden="true">
        <path d="M10 3 5 8l5 5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" fill="none" />
      </svg>
    </button>
  );
}

type BodyHeadProps = {
  title?: ReactNode;
  subtitle?: ReactNode;
  subtitleTone?: "default" | "danger";
  warning?: ReactNode;
};

function BodyHead({ title, subtitle, subtitleTone, warning }: BodyHeadProps) {
  return (
    <>
      {title != null && title !== false ? (
        <h1 className="assetaction__title">{title}</h1>
      ) : null}
      {subtitle != null && subtitle !== false ? (
        <div
          className={
            "assetaction__subtitle" +
            (subtitleTone === "danger" ? " assetaction__subtitle--danger" : "")
          }
        >
          {subtitle}
        </div>
      ) : null}
      {warning != null && warning !== false ? (
        <div className="assetaction__warningslot">{warning}</div>
      ) : null}
    </>
  );
}

type AssetActionLayoutProps = {
  media?: ReactNode;
  title?: ReactNode;
  subtitle?: ReactNode;
  subtitleTone?: "default" | "danger";
  warning?: ReactNode;
  icon?: ReactNode;
  iconTone?: string;
  onBack?: () => void;
  backLabel?: ReactNode;
  theme?: string;
  maxWidth?: number;
  center?: boolean;
  hideBack?: boolean;
  variant?: "row" | "status";
  bodyClassName?: string;
} & Omit<ComponentPropsWithoutRef<"div">, "title">;

export default function AssetActionLayout({
  media,
  children,
  title,
  subtitle,
  subtitleTone = "default",
  warning,
  icon,
  iconTone = "neutral",
  onBack,
  backLabel = "Back",
  theme = "light",
  maxWidth = 1100,
  center = false,
  hideBack = false,
  variant = "row",
  bodyClassName = "",
  className = "",
  ...rest
}: AssetActionLayoutProps) {
  const iconOnly = backLabel === null || backLabel === false;
  const isStatus = variant === "status";

  const back = !hideBack
    ? iconOnly
      ? <IconBack onBack={onBack} />
      : <TextBack label={backLabel} onBack={onBack} />
    : null;

  return (
    <div
      className={
        "assetaction assetaction--" + theme + (className ? " " + className : "")
      }
      {...rest}
    >
      <div
        className={
          "assetaction__page" + (isStatus ? " assetaction__page--status" : "")
        }
        style={{ maxWidth }}
      >
        {back}

        {isStatus ? (
          <div className="assetaction__statuscard">
            {icon != null && icon !== false ? (
              <div
                className={
                  "assetaction__statusicon assetaction__statusicon--" + iconTone
                }
                aria-hidden="true"
              >
                {icon}
              </div>
            ) : null}
            <BodyHead
              title={title}
              subtitle={subtitle}
              subtitleTone={subtitleTone}
              warning={warning}
            />
            {children}
          </div>
        ) : (
          <div className={"assetaction__row" + (center ? " assetaction__row--center" : "")}>
            <div className="assetaction__left">{media}</div>
            <div className={"assetaction__right" + (bodyClassName ? " " + bodyClassName : "")}>
              <BodyHead
                title={title}
                subtitle={subtitle}
                subtitleTone={subtitleTone}
                warning={warning}
              />
              {children}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
