import ManaMark from "../atoms/ManaMark";
import "./manapill.css";

type ManaPillProps = {
  value?: string;
  className?: string;
};

export default function ManaPill({ value = "2,480.55", className = "" }: ManaPillProps) {
  return (
    <span className={"u-manapill" + (className ? " " + className : "")} title="MANA balance">
      <ManaMark size={14} className="u-manapill__mark" />
      {value}
    </span>
  );
}
