import Button from "../../atoms/Button";
import "./fdembedviewport.css";

export type FdEmbedViewportProps = {
  url: string;
  title: string;
  /** What the visitor is about to download, in the world's own reported size. */
  weight?: string | null;
  started: boolean;
  onStart: () => void;
};

/**
 * The real client, in an iframe, on the visitor's explicit click.
 *
 * The `allow` list mirrors the live scene editor's viewport verbatim: the
 * `cross-origin-isolated` delegation plus the page's COOP/COEP headers are what
 * let the cross-origin engine reach shared memory. Nothing autoloads — the
 * engine and the scene are tens of megabytes, and a page that starts a download
 * nobody asked for is a page that lies about what a click costs.
 */
export default function FdEmbedViewport({
  url,
  title,
  weight = null,
  started,
  onStart,
}: FdEmbedViewportProps) {
  if (!started) {
    return (
      <div className="fd-embed fd-embed--idle">
        <p className="fd-embed__pitch">
          The game runs in the Decentraland web client, embedded here.
          {weight ? ` Starting it downloads the engine and this world's ${weight} of content.` : ""}
        </p>
        <Button variant="primary" size="md" onClick={onStart}>
          Load the game
        </Button>
      </div>
    );
  }

  return (
    <div className="fd-embed">
      <iframe
        className="fd-embed__frame"
        src={url}
        title={title}
        allow="cross-origin-isolated; autoplay; fullscreen; clipboard-read; clipboard-write; xr-spatial-tracking; gamepad; microphone; camera"
      />
    </div>
  );
}
