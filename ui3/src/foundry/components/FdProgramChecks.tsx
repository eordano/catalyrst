import FdScrollTable from "./FdScrollTable";
import "./fdprogramchecks.css";

export type FdProgramCheckVM = {
  id: string;
  label: string;
  /** Already formatted server-side; "—" when the table it counts is empty. */
  value: string;
  source: string;
};

type FdProgramChecksProps = {
  checks: readonly FdProgramCheckVM[];
  className?: string;
};

export default function FdProgramChecks({
  checks,
  className = "",
}: FdProgramChecksProps) {
  return (
    <FdScrollTable className={className} ariaLabel="Program checks">
      <table className="fd-table fd-checks">
        <thead>
          <tr>
            <th scope="col">Check</th>
            <th scope="col">Reading</th>
            <th scope="col">Where the number comes from</th>
          </tr>
        </thead>
        <tbody>
          {checks.map((check) => (
            <tr key={check.id}>
              <th scope="row">{check.label}</th>
              <td className="fd-table__num fd-checks__value">{check.value}</td>
              <td className="fd-checks__source">{check.source}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </FdScrollTable>
  );
}
