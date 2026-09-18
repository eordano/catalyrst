import type { ComponentPropsWithoutRef, ReactNode } from "react";
import "./surface.css";

export function PageHeader({ title, description, actions, children, className = "", ...rest }: Omit<ComponentPropsWithoutRef<"header">, "title"> & {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return <header className={`ui-page-header ${className}`} {...rest}>
    <div className="ui-page-header__heading"><h1>{title}</h1>{description && <p>{description}</p>}</div>
    {children}
    {actions && <div className="ui-page-header__actions">{actions}</div>}
  </header>;
}

export function FilterButton({ selected = false, className = "", children, ...rest }: ComponentPropsWithoutRef<"button"> & { selected?: boolean }) {
  return <button type="button" aria-pressed={selected} className={`ui-filter${selected ? " is-active" : ""} ${className}`} {...rest}>{children}</button>;
}

export function SurfaceCard({ className = "", children, ...rest }: ComponentPropsWithoutRef<"article">) {
  return <article className={`ui-card ${className}`} {...rest}>{children}</article>;
}

export function SectionCard({ title, titleId, icon, action, footer, children, className = "", ...rest }: Omit<ComponentPropsWithoutRef<"section">, "title"> & {
  title: ReactNode;
  titleId: string;
  icon?: ReactNode;
  action?: ReactNode;
  footer?: ReactNode;
}) {
  return <section className={`ui-section-card ${className}`} aria-labelledby={titleId} {...rest}>
    <header><h2 id={titleId}>{icon}{title}</h2>{action}</header>
    <div className="ui-section-card__body">{children}</div>
    {footer && <footer>{footer}</footer>}
  </section>;
}
