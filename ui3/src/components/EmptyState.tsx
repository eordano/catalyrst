import type {
  ComponentPropsWithoutRef,
  CSSProperties,
  ElementType,
  MouseEventHandler,
  ReactNode,
} from "react";
import "./emptystate.css";

type ActionVariant = "solid" | "outline" | "ghost";

const CTA_VARIANT: Record<ActionVariant, string> = {
  solid: "es__cta",
  outline: "es__cta es__cta--outline",
  ghost: "es__cta es__cta--ghost",
};

export type EmptyStateAction = {
  label?: ReactNode;
  onClick?: MouseEventHandler<HTMLElement>;
  href?: string;
  variant?: ActionVariant;
  icon?: ReactNode;
};

function Action({ label, onClick, href, variant = "solid", icon, ...rest }: EmptyStateAction) {
  const cls = CTA_VARIANT[variant] || CTA_VARIANT.solid;
  const inner = (
    <>
      {icon ? <span className="es__ctaicon" aria-hidden="true">{icon}</span> : null}
      {label}
    </>
  );
  if (href != null) {
    return (
      <a className={cls} href={href} onClick={onClick} {...rest}>
        {inner}
      </a>
    );
  }
  return (
    <button type="button" className={cls} onClick={onClick} {...rest}>
      {inner}
    </button>
  );
}

const len = (v: number | string): string => (typeof v === "number" ? v + "px" : v);

type EmptyStateVariant = "inline" | "screen";

const VARIANT_CLASS: Record<EmptyStateVariant, string> = {
  inline: " es--inline",
  screen: " es--screen",
};

type EmptyStateStyle = CSSProperties & { "--es-actions-gap"?: string };

export type EmptyStateProps = {
  icon?: ReactNode;
  iconWash?: boolean;
  title?: ReactNode;
  titleAs?: ElementType;
  subtitle?: ReactNode;
  actions?: ReactNode | readonly EmptyStateAction[];
  variant?: EmptyStateVariant;
  tone?: "error";
  actionsGap?: number | string;
} & Omit<ComponentPropsWithoutRef<"div">, "title">;

function isActionList(a: ReactNode | readonly EmptyStateAction[]): a is readonly EmptyStateAction[] {
  return Array.isArray(a);
}

export default function EmptyState({
  icon,
  iconWash = false,
  title,
  titleAs: TitleTag = "h2",
  subtitle,
  actions,
  variant,
  tone,
  actionsGap,
  className = "",
  style,
  ...rest
}: EmptyStateProps) {
  const cls =
    "es" +
    (variant ? VARIANT_CLASS[variant] : "") +
    (tone === "error" ? " es--error" : "") +
    (className ? " " + className : "");

  const gap = actionsGap != null ? actionsGap : tone === "error" ? 40 : undefined;
  const mergedStyle: EmptyStateStyle | undefined =
    gap != null ? { ...style, "--es-actions-gap": len(gap) } : style;

  return (
    <div className={cls} style={mergedStyle} {...rest}>
      {icon != null ? (
        <div
          className={"es__icon" + (iconWash ? " es__icon--wash" : "")}
          aria-hidden="true"
        >
          {icon}
        </div>
      ) : null}

      {title != null ? <TitleTag className="es__title">{title}</TitleTag> : null}
      {subtitle != null ? <p className="es__sub">{subtitle}</p> : null}

      {isActionList(actions) ? (
        actions.length ? (
          <div className="es__actions">
            {actions.map((a, i) => (
              <Action key={i} {...a} />
            ))}
          </div>
        ) : null
      ) : actions != null ? (
        <div className="es__actions">{actions}</div>
      ) : null}
    </div>
  );
}
