import { useEffect, useRef } from "react";
import { Link, useRevalidator, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import Button from "@ui/atoms/Button";
import EmptyState from "@ui/components/EmptyState";
import StStorageSelect from "@ui/web/pages/StStorageSelect";
import StStorageScene from "@ui/web/pages/StStorageScene";
import StStorageEnvironment from "@ui/web/pages/StStorageEnvironment";
import StStoragePlayers from "@ui/web/pages/StStoragePlayers";
import ChModalWorldsYourStorage from "@ui/creatorhub/components/ChModalWorldsYourStorage";
import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import {
  clearEnvKeys,
  clearValues,
  coerceStep,
  deleteEnvKey,
  deleteValue,
  findWorld,
  saveEnvKey,
  saveValue,
  type EnvKey,
  type StorageLand,
  type StorageStep,
  type StorageValue,
  type StorageWorld,
  type WorldsStorageData,
  type WriteScope,
} from "@data/lib/catalyst/creator-hub/worlds-storage";
import { loadWorldsStorage } from "@data/lib/catalyst/creator-hub/worlds-storage.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.worlds-storage";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Worlds storage");

const STORY: StoryId = "creator-hub/worlds-storage";

const NAMESPACES = ["scene", "env", "players"] as const;
type Namespace = (typeof NAMESPACES)[number];

function coerceNamespace(raw: string | null | undefined): Namespace {
  return (NAMESPACES as readonly string[]).includes(raw ?? "")
    ? (raw as Namespace)
    : "scene";
}

const FALLBACK: Assignment = {
  variant: "storage-dashboard",
  flags: { showQuotaModal: true },
  experimentKey: "creator-hub-worlds-storage",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address =
    url.searchParams.get("address")?.trim() || readWallet(request) || undefined;
  const step = coerceStep(url.searchParams.get("step"));
  const namespace = coerceNamespace(url.searchParams.get("namespace"));
  const worldParam = url.searchParams.get("world")?.trim() ?? "";
  const key = url.searchParams.get("key")?.trim() ?? "";
  const quotaOpen = url.searchParams.get("quota") === "1";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const d = await loadWorldsStorage(address, { signal: request.signal }).catch(
    () => null,
  );

  const worlds = d?.worlds ?? [];
  const selected =
    findWorld(worlds, worldParam) ??
    (step !== "select" ? (worlds[0] ?? null) : null);

  const payload = {
    sid,
    address: address ?? "",
    step,
    namespace,
    world: selected?.name ?? worldParam ?? null,
    key,
    quotaOpen,
    source: d?.source ?? "empty",
    fallback: d?.fallback ?? (d == null),
    worlds,
    lands: d?.lands ?? [],
    stats: d?.stats ?? null,
    scope: d?.scope ?? null,
    values: d?.values ?? [],
    envKeys: d?.envKeys ?? [],
    players: d?.players ?? { addresses: [], profileNames: {} },
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  address: string;
  step: StorageStep;
  namespace: Namespace;
  world: string | null;
  key: string;
  quotaOpen: boolean;
  source: "live" | "empty";
  fallback: boolean;
  worlds: StorageWorld[];
  lands: StorageLand[];
  stats: WorldsStorageData["stats"] | null;
  scope: WorldsStorageData["scope"] | null;
  values: StorageValue[];
  envKeys: EnvKey[];
  players: WorldsStorageData["players"];
};

export default function CreatorHubWorldsStorage({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  const [, setSearchParams] = useSearchParams();

  const rescoping = isConnected && Boolean(address) && !d.address;
  const needsConnect = !d.address && !rescoping;

  useEffect(() => {
    if (isConnected && address && !d.address) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("address", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, d.address, setSearchParams]);

  return (
    <CreatorHubChrome
      active="manage"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
    >
      <CreatorHubBreadcrumb to={href("/creator-hub/manage")} label="Back to Worlds" LinkComponent={Link} />

      {rescoping ? (
        <div
          role="status"
          aria-live="polite"
          style={{ display: "flex", alignItems: "center", gap: 10 }}
        >
          <span className="u-spinner" aria-hidden="true" />
          <span style={{ color: "var(--ink-45)", fontSize: 14 }}>
            Loading your storage&#x2026;
          </span>
        </div>
      ) : needsConnect ? (
        <EmptyState
          className="es--card"
          title="Sign in to manage your storage"
          subtitle="Sign in to see your worlds and land parcels and manage their storage. World storage grows with what you hold: 100 Mb per 2,000 MANA, per LAND, and per NAME (a new NAME costs 100 MANA)."
          actions={
            <Button variant="secondary" onClick={() => openSignIn()}>
              Sign in
            </Button>
          }
          icon={undefined}
          iconWash={undefined}
          variant={undefined}
          tone={undefined}
          actionsGap={undefined}
          style={undefined}
        />
      ) : (
        <WorldsStorageView data={d} />
      )}
    </CreatorHubChrome>
  );
}

function WorldsStorageView({ data: d }: { data: LoaderData }) {
  const [, setSearchParams] = useSearchParams();
  const { identity } = useAuth();
  const revalidator = useRevalidator();

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const k = `${d.step}|${d.namespace}|${d.world}|${d.quotaOpen}`;
    if (lastKey.current === k) return;
    lastKey.current = k;
    track(
      "ch_worlds_storage_viewed",
      {
        step: d.step,
        namespace: d.namespace,
        world: d.world,
        source: d.source,
        fallback: d.fallback,
        worlds: d.worlds.length,
        lands: d.lands.length,
      },
      { sid: d.sid, story: STORY },
    );
    if (d.quotaOpen) {
      track(
        "ch_worlds_storage_quota_opened",
        { world: d.world },
        { sid: d.sid, story: STORY },
      );
    }
  }, [d.sid, d.step, d.namespace, d.world, d.quotaOpen, d.source, d.fallback, d.worlds.length, d.lands.length]);

  function setParams(patch: Record<string, string>) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(patch)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        return next;
      },
      { preventScrollReset: true },
    );
  }

  const quotaPanel = !d.quotaOpen ? null : d.stats ? (
    <ChModalWorldsYourStorage
      variant="panel"
      embedded
      open
      stats={{
        usedSpace: String(d.stats.usedSpace),
        maxAllowedSpace: String(d.stats.maxAllowedSpace),
      }}
      manaHref="https://account.decentraland.org/"
      landHref="https://decentraland.org/marketplace/lands"
      nameHref="/creator-hub/claim-name"
      learnMoreHref="https://docs.decentraland.org/creator/worlds/about/"
      onClose={() => setParams({ quota: "" })}
    />
  ) : (
    <div
      role="note"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 16,
        flexWrap: "wrap",
        border: "1px solid var(--line)",
        borderRadius: "var(--r-control)",
        background: "var(--fill-2)",
        color: "var(--ink-85)",
        padding: "10px 14px",
        margin: "0 0 14px",
        fontSize: 13,
      }}
    >
      <span>
        <strong>Your Storage</strong> &#x2014; we couldn&apos;t load your storage
        usage right now, so the breakdown can&apos;t be shown. Your worlds and
        published scenes are not affected. Storage grows with the MANA, LAND,
        and NAMEs you own &#x2014;{" "}
        <a
          href="https://docs.decentraland.org/creator/worlds/about/"
          target="_blank"
          rel="noreferrer"
          style={{ color: "inherit", textDecoration: "underline" }}
        >
          learn how world storage works
        </a>
        .
      </span>
      <span style={{ display: "inline-flex", gap: 8 }}>
        <button
          type="button"
          disabled={revalidator.state !== "idle"}
          onClick={() => {
            track(
              "ch_worlds_storage_quota_retry",
              { world: d.world },
              { sid: d.sid, story: STORY },
            );
            revalidator.revalidate();
          }}
          style={{
            border: "1px solid var(--line)",
            background: "transparent",
            color: "inherit",
            borderRadius: "var(--r-control)",
            padding: "4px 12px",
            fontWeight: 600,
            cursor: revalidator.state !== "idle" ? "default" : "pointer",
          }}
        >
          {revalidator.state !== "idle" ? "Retrying\u{2026}" : "Try again"}
        </button>
        <button
          type="button"
          onClick={() => setParams({ quota: "" })}
          style={{
            border: "1px solid var(--line)",
            background: "transparent",
            color: "inherit",
            borderRadius: "var(--r-control)",
            padding: "4px 12px",
            cursor: "pointer",
          }}
        >
          Close
        </button>
      </span>
    </div>
  );

  const storageBar = d.quotaOpen ? null : (
    <div style={{ margin: "0 0 14px" }}>
      <button
        type="button"
        onClick={() => setParams({ quota: "1" })}
        style={{
          border: "1px solid var(--line)",
          background: "transparent",
          color: "var(--ink-85)",
          borderRadius: "var(--r-control)",
          padding: "6px 12px",
          fontSize: 13,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        Your Storage
      </button>
    </div>
  );

  if (d.step === "select") {
    return (
      <>
        {storageBar}
        {quotaPanel}
        <SelectStep
        sid={d.sid}
        worlds={d.worlds}
        lands={d.lands}
        onPickWorld={(name) => {
          track(
            "ch_worlds_storage_asset_selected",
            { world: name, kind: "world" },
            { sid: d.sid, story: STORY },
          );
          setParams({ step: "scene", world: name, namespace: "" });
        }}
        onPickLand={(land) => {
          track(
            "ch_worlds_storage_asset_selected",
            { world: land.name, kind: "land" },
            { sid: d.sid, story: STORY },
          );
          setParams({ step: "scene", world: land.name, namespace: "" });
        }}
        />
      </>
    );
  }

  const realm = d.world ?? d.scope?.realm ?? "vitsky.dcl.eth";
  const position = d.scope?.position ?? "0,0";
  const scope: WriteScope = { realm, parcel: position };
  const trackCtx = { sid: d.sid, story: STORY };

  function onNamespaceClick(e: React.MouseEvent<HTMLDivElement>) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[role="tab"]');
    if (!el) return;
    const label = (el.textContent ?? "").toLowerCase();
    const next: Namespace = label.includes("environment")
      ? "env"
      : label.includes("player")
        ? "players"
        : "scene";
    if (next === d.namespace) return;
    track(
      "ch_worlds_storage_namespace_changed",
      { namespace: next, world: d.world },
      { sid: d.sid, story: STORY },
    );
    setParams({ namespace: next === "scene" ? "" : next });
  }

  if (d.step === "edit" && d.namespace !== "players") {
    const mode = d.key ? "edit" : "add";
    return (
      <DialogStep
        sid={d.sid}
        world={d.world}
        namespace={d.namespace}
        mode={mode}
        editKey={d.key || null}
        realm={realm}
        position={position}
        values={d.values}
        envKeys={d.envKeys}
        onClose={() => setParams({ step: "scene", key: "" })}
        onSaved={async (payload) => {
          const label = {
            mode,
            key: d.key || null,
            world: d.world,
            namespace: d.namespace,
          };
          if (!identity) {
            openSignIn();
            track(
              "ch_worlds_storage_value_saved",
              { ...label, stub: true, reason: "not_connected" },
              trackCtx,
            );
            return;
          }
          if (!payload) {
            track(
              "ch_worlds_storage_value_saved",
              { ...label, stub: true, reason: "no_value" },
              trackCtx,
            );
            setParams({ step: "scene", key: "" });
            return;
          }
          try {
            if (d.namespace === "env") {
              await saveEnvKey(payload.key, payload.value, { identity, scope });
            } else {
              await saveValue(payload.key, coerceSceneValue(payload.value), {
                identity,
                scope,
              });
            }
            track("ch_worlds_storage_value_saved", label, trackCtx);
            setParams({ step: "scene", key: "" });
          } catch (err) {
            track(
              "ch_worlds_storage_value_save_failed",
              { ...label, error: String(err) },
              trackCtx,
            );
          }
        }}
      />
    );
  }

  if (d.step === "clear" && d.namespace !== "players") {
    const count =
      d.namespace === "env" ? d.envKeys.length : d.values.length;
    return (
      <div onClickCapture={onNamespaceClick}>
        {storageBar}
        {quotaPanel}
        <NamespaceTable
          namespace={d.namespace}
          realm={realm}
          position={position}
          values={d.values}
          envKeys={d.envKeys}
          players={d.players}
          initialDialog="clear"
          onEditKey={(k) => {
            track(
              "ch_worlds_storage_dialog_opened",
              { mode: "edit", key: k, world: d.world },
              { sid: d.sid, story: STORY },
            );
            setParams({ step: "edit", key: k });
          }}
          onAdd={() => {
            track(
              "ch_worlds_storage_dialog_opened",
              { mode: "add", key: null, world: d.world },
              { sid: d.sid, story: STORY },
            );
            setParams({ step: "edit", key: "" });
          }}
          onClearConfirmed={async () => {
            const label = { world: d.world, namespace: d.namespace, count };
            if (!identity) {
              openSignIn();
              track(
                "ch_worlds_storage_cleared",
                { ...label, stub: true, reason: "not_connected" },
                trackCtx,
              );
              return;
            }
            try {
              if (d.namespace === "env") {
                await clearEnvKeys({ identity, scope });
              } else {
                await clearValues({ identity, scope });
              }
              track("ch_worlds_storage_cleared", label, trackCtx);
              setParams({ step: "scene" });
            } catch (err) {
              track(
                "ch_worlds_storage_clear_failed",
                { ...label, error: String(err) },
                trackCtx,
              );
            }
          }}
          onClearCancel={() => setParams({ step: "scene" })}
        />
      </div>
    );
  }

  return (
    <div onClickCapture={onNamespaceClick}>
      {storageBar}
      {quotaPanel}
      <NamespaceTable
        namespace={d.namespace}
        realm={realm}
        position={position}
        values={d.values}
        envKeys={d.envKeys}
        players={d.players}
        initialDialog={null}
        onEditKey={(k) => {
          track(
            "ch_worlds_storage_dialog_opened",
            { mode: "edit", key: k, world: d.world },
            { sid: d.sid, story: STORY },
          );
          setParams({ step: "edit", key: k });
        }}
        onAdd={() => {
          track(
            "ch_worlds_storage_dialog_opened",
            { mode: "add", key: null, world: d.world },
            { sid: d.sid, story: STORY },
          );
          setParams({ step: "edit", key: "" });
        }}
        onDeleteKey={async (k) => {
          const label = { key: k, world: d.world, namespace: d.namespace };
          if (!identity) {
            openSignIn();
            track(
              "ch_worlds_storage_value_deleted",
              { ...label, stub: true, reason: "not_connected" },
              trackCtx,
            );
            return;
          }
          try {
            if (d.namespace === "env") {
              await deleteEnvKey(k, { identity, scope });
            } else {
              await deleteValue(k, { identity, scope });
            }
            track("ch_worlds_storage_value_deleted", label, trackCtx);
          } catch (err) {
            track(
              "ch_worlds_storage_value_delete_failed",
              { ...label, error: String(err) },
              trackCtx,
            );
          }
        }}
        onClear={() => setParams({ step: "clear" })}
      />
    </div>
  );
}

type SelectStepProps = {
  sid: string;
  worlds: StorageWorld[];
  lands: StorageLand[];
  onPickWorld: (name: string) => void;
  onPickLand: (land: StorageLand) => void;
};

function SelectStep({ worlds, lands, onPickWorld, onPickLand }: SelectStepProps) {
  const uiWorlds = worlds.map((w) => ({
    name: w.name,
    role: w.role,
    scenes: w.scenes,
  }));
  const uiLands = lands.map((l) => ({ id: l.id, name: l.name, role: l.role }));

  function onClick(e: React.MouseEvent<HTMLDivElement>) {
    const el = e.target as HTMLElement;

    const editBtn = el.closest<HTMLElement>(".ststorageselect__btn");
    if (editBtn) {
      const card = editBtn.closest<HTMLElement>(".ststorageselect__card");
      const name = card
        ?.querySelector(".ststorageselect__cardname")
        ?.textContent?.trim();
      const world = uiWorlds.find((w) => w.name === name);
      if (world) {
        onPickWorld(world.name);
        return;
      }
    }

    const landCard = el.closest<HTMLElement>(".ststorageselect__card--action");
    if (landCard) {
      const name = landCard
        .querySelector(".ststorageselect__landname")
        ?.textContent?.trim();
      const land = uiLands.find((l) => l.name === name);
      if (land) onPickLand(land);
    }
  }

  return (
    <div onClick={onClick}>
      <StStorageSelect worlds={uiWorlds} lands={uiLands} embedded />
    </div>
  );
}

type NamespaceTableProps = {
  namespace: Namespace;
  realm: string;
  position: string;
  values: StorageValue[];
  envKeys: EnvKey[];
  players: WorldsStorageData["players"];
  initialDialog: "add" | "clear" | null;
  onEditKey: (key: string) => void;
  onAdd: () => void;
  onDeleteKey?: (key: string) => void;
  onClear?: () => void;
  onClearConfirmed?: () => void;
  onClearCancel?: () => void;
};

function NamespaceTable({
  namespace,
  realm,
  position,
  values,
  envKeys,
  players,
  initialDialog,
  onEditKey,
  onAdd,
  onDeleteKey,
  onClear,
  onClearConfirmed,
  onClearCancel,
}: NamespaceTableProps) {
  function onClick(e: React.MouseEvent<HTMLDivElement>) {
    const btn = (e.target as HTMLElement).closest<HTMLElement>("button");
    if (!btn) return;
    const aria = (btn.getAttribute("aria-label") ?? "").toLowerCase();
    const text = (btn.textContent ?? "").trim().toLowerCase();

    if (aria.startsWith("edit ")) {
      onEditKey(aria.slice("edit ".length));
      return;
    }
    if (aria.startsWith("delete ")) {
      onDeleteKey?.(aria.slice("delete ".length));
      return;
    }
    if (text === "add") {
      onAdd();
      return;
    }
    if (text.startsWith("clear all")) {
      onClear?.();
      return;
    }
    if (initialDialog === "clear") {
      if (aria === "confirm" || text === "confirm") onClearConfirmed?.();
      else if (aria === "cancel" || text === "cancel") onClearCancel?.();
    }
  }

  if (namespace === "players") {
    return (
      <StStoragePlayers
        realm={realm}
        position={position as unknown as null}
        players={players.addresses}
        profileNames={new Map(Object.entries(players.profileNames))}
        embedded
      />
    );
  }

  if (namespace === "env") {
    return (
      <div onClick={onClick}>
        <StStorageEnvironment
          envKeys={envKeys}
          scope={{ realm, position }}
          activeTab="env"
          embedded
        />
      </div>
    );
  }

  const sceneKeys = values.map((v) => ({ key: v.key }));
  return (
    <div onClick={onClick}>
      <StStorageScene
        sceneKeys={sceneKeys}
        realm={realm}
        position={position}
        initialDialog={(initialDialog === "clear" ? null : initialDialog) as null | undefined}
        embedded
      />
    </div>
  );
}

type SavePayload = { key: string; value: string } | null;

function coerceSceneValue(raw: string): unknown {
  const t = raw.trim();
  if (t.startsWith("{") || t.startsWith("[")) {
    try {
      return JSON.parse(t);
    } catch {
      return raw;
    }
  }
  return raw;
}

function readDialogPayload(
  saveBtn: HTMLElement,
  mode: "add" | "edit",
  editKey: string | null,
): SavePayload {
  const card = saveBtn.closest<HTMLElement>(".ss-dialog") ?? undefined;
  const valueEl = card?.querySelector<HTMLTextAreaElement>(".ss-field__textarea");
  if (!valueEl) return null;
  const key =
    mode === "edit"
      ? (editKey ?? "")
      : (card
          ?.querySelector<HTMLInputElement>(".ss-field__input")
          ?.value.trim() ?? "");
  if (!key) return null;
  return { key, value: valueEl.value };
}

type DialogStepProps = {
  sid: string;
  world: string | null;
  namespace: Namespace;
  mode: "add" | "edit";
  editKey: string | null;
  realm: string;
  position: string;
  values: StorageValue[];
  envKeys: EnvKey[];
  onClose: () => void;
  onSaved: (payload: SavePayload) => void;
};

function DialogStep({
  mode,
  editKey,
  realm,
  position,
  values,
  onClose,
  onSaved,
}: DialogStepProps) {
  function onClick(e: React.MouseEvent<HTMLDivElement>) {
    const btn = (e.target as HTMLElement).closest<HTMLElement>("button");
    if (!btn) return;
    const text = (btn.textContent ?? "").trim().toLowerCase();
    if (text === "save") onSaved(readDialogPayload(btn, mode, editKey));
    else if (text === "cancel") onClose();
  }

  const sceneKeys =
    mode === "edit" && editKey
      ? [{ key: editKey }, ...values.filter((v) => v.key !== editKey).map((v) => ({ key: v.key }))]
      : values.map((v) => ({ key: v.key }));

  return (
    <div onClick={onClick}>
      <StStorageScene
        sceneKeys={sceneKeys}
        realm={realm}
        position={position}
        initialDialog={mode as unknown as null}
        embedded
      />
    </div>
  );
}
