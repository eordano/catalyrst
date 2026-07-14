import "./deploytargethealthview.css";

type DeployTargetHealthViewProps = {
  online?: boolean | null;
};

export default function DeployTargetHealthView({
  online = null,
}: DeployTargetHealthViewProps) {
  const down = online === false;
  const ok = online === true;
  return (
    <aside className="deploy-target-health" aria-label="Worlds server status">
      <header className="deploy-target-health__head">
        <strong className="deploy-target-health__title">Worlds server</strong>
        <span
          className={
            "deploy-target-health__status" +
            (down
              ? " deploy-target-health__status--down"
              : ok
                ? " deploy-target-health__status--ok"
                : "")
          }
        >
          {down ? "Unavailable" : ok ? "Online" : "Status unknown"}
        </span>
      </header>

      {down ? (
        <p role="alert" className="deploy-target-health__body">
          We couldn&rsquo;t reach the Worlds server that receives published
          scenes. Publishing will most likely fail right now &mdash; try again in
          a few minutes.
        </p>
      ) : ok ? (
        <p className="deploy-target-health__body">
          The Worlds server that receives published scenes is up and ready for
          your World.
        </p>
      ) : (
        <p className="deploy-target-health__body">
          We couldn&rsquo;t confirm the Worlds server status. You can still try
          to publish your World as usual.
        </p>
      )}
    </aside>
  );
}
