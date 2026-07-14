import Button from "../../atoms/Button";
import "./fdrolecard.css";

export type FdRoleId = "start" | "create" | "admin";

export type FdRoleCardVM = {
  id: FdRoleId;
  role: string;
  who: string;
  title: string;
  body: string;
  destination: string;
  href: string;
  cta: string;
};

/** A one-line reading from the database rendered on the door — e.g. how many
 *  games are live. Absent renders exactly the static door. */
export type FdRoleStateLine = { text: string; href: string; title?: string };

export type FdRoleCardProps = {
  role: FdRoleCardVM;
  stateLine?: FdRoleStateLine | null;
  onChoose?: (id: FdRoleId) => void;
};

export default function FdRoleCard({
  role,
  stateLine = null,
  onChoose,
}: FdRoleCardProps) {
  return (
    <li className="fd-card fd-role">
      <p className="fd-role__role">
        <span className="fd-label fd-label--eyebrow">{role.role}</span>
        <span className="fd-role__who">{role.who}</span>
      </p>
      <h3 className="fd-card__title">{role.title}</h3>
      <p className="fd-role__body">{role.body}</p>
      <p className="fd-role__dest">{role.destination}</p>
      {stateLine ? (
        <p className="fd-role__state">
          <a href={stateLine.href} title={stateLine.title}>
            {stateLine.text}
          </a>
        </p>
      ) : null}
      <Button
        as="a"
        variant="primary"
        size="sm"
        className="fd-cardlink fd-role__cta"
        href={role.href}
        onClick={() => onChoose?.(role.id)}
      >
        {role.cta}
      </Button>
    </li>
  );
}
