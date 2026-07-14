import "./fdrolecard.css";

export type FdRoleId = "start" | "create" | "admin";

export type FdRoleCardVM = {
  id: FdRoleId;
  role: string;
  who: string;
  title: string;
  body: string;
  blurLine: string;
  destination: string;
  href: string;
  cta: string;
};

export type FdRoleCardProps = {
  role: FdRoleCardVM;
  sameLine: string;
  onChoose?: (id: FdRoleId) => void;
};

export default function FdRoleCard({ role, sameLine, onChoose }: FdRoleCardProps) {
  return (
    <li className="fd-role">
      <p className="fd-role__role">
        <span className="fd-role__name">{role.role}</span>
        <span className="fd-role__who">{role.who}</span>
      </p>
      <h3 className="fd-role__title">{role.title}</h3>
      <p className="fd-role__body">{role.body}</p>
      <p className="fd-role__blur">{role.blurLine}</p>
      <p className="fd-role__dest">
        {role.destination}
        <span className="fd-role__same">{sameLine}</span>
      </p>
      <a
        className="fd-role__cta"
        href={role.href}
        onClick={() => onChoose?.(role.id)}
      >
        {role.cta}
      </a>
    </li>
  );
}
