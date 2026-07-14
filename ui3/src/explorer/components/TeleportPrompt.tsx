import "./teleportprompt.css";

type TeleportPromptProps = {
  showActions?: boolean;
};

export default function TeleportPrompt({ showActions = true }: TeleportPromptProps) {
  const destination = "Place's name";
  return (
    <div className="tp" role="dialog" aria-modal="true" aria-label={`Jump in to ${destination}`}>
      <div className="tp__image" aria-hidden="true" />

      <div className="tp__body">
        <h2 className="tp__name">{destination}</h2>
        <p className="tp__creator">created by <span>creator</span></p>

        {showActions && (
          <div className="tp__actions">
            <button className="tp__cancel">CANCEL</button>
            <button className="tp__jump" data-sb-linkto="Explorer/Workflows/SceneLoading">JUMP IN</button>
          </div>
        )}
      </div>
    </div>
  );
}
