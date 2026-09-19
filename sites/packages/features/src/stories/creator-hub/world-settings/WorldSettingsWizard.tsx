import { useEffect, useRef, useState } from "react";
import { useBlocker, useRevalidator, useSearchParams } from "react-router";
import ChWorldSettingsTabbedSections, {
  type WorldSceneVM, type WorldSettingsValues,
} from "@ui/creatorhub/components/ChWorldSettingsTabbedSections";
import { useAuth } from "@data/lib/auth/index";
import { unpublishWorldScene } from "@data/lib/catalyst/creator-hub/unpublish-scene";
import { saveWorldSettings, settingsForm, type WorldSettingsChanges } from "@data/lib/catalyst/creator-hub/world-settings";
import { track, type TrackContext } from "@core/lib/telemetry/track";

export type { WorldSceneVM };

type Props = {
  trackCtx: TrackContext;
  worldName: string;
  settings: WorldSettingsValues;
  initialStep?: string;
  scenes?: WorldSceneVM[];
  isOwner?: boolean;
  onClose?: () => void;
};

export default function WorldSettingsWizard({
  trackCtx, worldName, settings, initialStep, scenes, isOwner = false, onClose,
}: Props) {
  const [params, setParams] = useSearchParams();
  const step = params.get("step") || initialStep;
  const tab = step === "layout" ? "layout" : step === "misc" || step === "general" ? "general" : "details";
  const [saved, setSaved] = useState(settings);
  const [draft, setDraft] = useState(settings);
  const [thumbnail, setThumbnail] = useState<File>();
  const [thumbnailUrl, setThumbnailUrl] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [unpublishingCoord, setUnpublishingCoord] = useState<string | null>(null);
  const { identity } = useAuth();
  const revalidator = useRevalidator();
  const pending = useRef<AbortController | null>(null);
  const opened = useRef(false);
  useEffect(() => {
    if (!opened.current) {
      opened.current = true;
      track("ch_world_settings_opened", { world: worldName }, trackCtx);
    }
  }, [trackCtx, worldName]);
  useEffect(() => {
    track("ch_world_settings_tab_viewed", { tab: tab === "general" ? "misc" : tab, world: worldName }, trackCtx);
  }, [tab, worldName]);
  const changes = Object.fromEntries(Object.entries(draft).filter(([key, value]) =>
    key !== "thumbnailUrl" && JSON.stringify(value) !== JSON.stringify(saved[key as keyof WorldSettingsValues]),
  )) as WorldSettingsChanges;
  const dirty = Boolean(thumbnail) || Object.keys(changes).length > 0;
  const blocker = useBlocker(({ currentLocation, nextLocation }) => dirty && !saving && (
    currentLocation.pathname !== nextLocation.pathname ||
    new URLSearchParams(currentLocation.search).get("world") !== new URLSearchParams(nextLocation.search).get("world")
  ));

  useEffect(() => {
    if (blocker.state !== "blocked") return;
    if (window.confirm("Discard your unsaved World settings?")) blocker.proceed();
    else blocker.reset();
  }, [blocker]);
  useEffect(() => {
    if (!dirty) return;
    const beforeUnload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ""; };
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [dirty]);
  useEffect(() => () => pending.current?.abort(), []);
  useEffect(() => {
    if (!thumbnail) { setThumbnailUrl(undefined); return; }
    const url = URL.createObjectURL(thumbnail);
    setThumbnailUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [thumbnail]);

  function discard() {
    track("ch_world_settings_discarded", { world: worldName, change_count: Object.keys(changes).length + Number(Boolean(thumbnail)) }, trackCtx);
    setDraft(saved); setThumbnail(undefined); setError(null); setMessage(null);
  }

  async function save() {
    if (!isOwner || !dirty || pending.current) return;
    if (!identity) { setError("Connect your wallet to save World settings."); return; }
    const controller = new AbortController();
    pending.current = controller;
    setSaving(true); setError(null); setMessage(null);
    track("ch_world_settings_saving", { world: worldName, fields: Object.keys(changes) }, trackCtx);
    try {
      const next = await saveWorldSettings(worldName, changes, { identity, thumbnail, signal: controller.signal });
      if (controller.signal.aborted) return;
      setSaved(next); setDraft(next); setThumbnail(undefined);
      setMessage("World settings saved.");
      track("ch_world_settings_saved", { world: worldName, fields: [...Object.keys(changes), ...(thumbnail ? ["thumbnail"] : [])], stub: false }, trackCtx);
    } catch (err) {
      if (!controller.signal.aborted) setError(err instanceof Error ? err.message : "Could not save World settings. Try again.");
    } finally {
      pending.current = null;
      if (!controller.signal.aborted) setSaving(false);
    }
  }

  async function unpublish(coord: string) {
    if (!identity) { setError("Connect your wallet to remove a scene."); return; }
    setError(null); setUnpublishingCoord(coord);
    try {
      await unpublishWorldScene(worldName, coord, { identity });
      await revalidator.revalidate();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not remove the scene. Try again.");
    } finally { setUnpublishingCoord(null); }
  }

  return (
    <div className="world-settings-wizard" data-step={tab}>
      <ChWorldSettingsTabbedSections
        variant="panel" tab={tab} isOwner={isOwner} isLoading={saving}
        hasChanges={isOwner && dirty} layoutView="scenes" worldName={worldName} scenes={scenes}
        settings={{ ...draft, thumbnailUrl: thumbnailUrl ?? draft.thumbnailUrl }}
        onSettingsChange={(change) => {
          setDraft((current) => ({ ...current, ...change })); setMessage(null);
          for (const field of Object.keys(change)) track("ch_world_settings_changed", { world: worldName, tab: tab === "general" ? "misc" : tab, field }, trackCtx);
        }}
        onThumbnail={(file) => {
          try { settingsForm({}, file); setThumbnail(file); setError(null); setMessage(null); }
          catch (err) { setError(err instanceof Error ? err.message : "Could not load that image."); }
        }}
        onUnpublish={unpublish} unpublishingCoord={unpublishingCoord}
        onClose={onClose}
        onTabChange={(next) => setParams((current) => {
          const query = new URLSearchParams(current);
          query.set("step", next === "general" ? "misc" : next);
          return query;
        }, { replace: true, preventScrollReset: true })}
        onDiscard={discard} onSave={save}
      />
      {saving && <p role="status">Saving World settings&#x2026;</p>}
      {message && <p role="status">{message}</p>}
      {error && <p className="world-settings-wizard__error" role="alert">{error}</p>}
    </div>
  );
}
