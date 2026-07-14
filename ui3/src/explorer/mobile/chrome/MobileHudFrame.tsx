import type { ReactNode } from "react";
import MobileActionCluster from "./MobileActionCluster";
import MobileMinimapPuck from "./MobileMinimapPuck";
import MobileTabBar from "./MobileTabBar";
import MobileTopBar from "./MobileTopBar";
import type { MobileAction, MobileOrientation, MobileTab } from "./types";
import "./mobilehudframe.css";

export type MobileHudFrameProps = {
  orientation?: MobileOrientation;
  place?: string;
  coords?: string;
  heading?: number;
  mapSrc?: string | null;
  mapCollapsed?: boolean;
  unread?: number;
  crosshair?: boolean;
  hudHidden?: boolean;
  tabs?: MobileTab[];
  activeTab?: string;
  actions?: MobileAction[];
  mainAction?: string;
  actionsHidden?: boolean;
  controlsSlot?: ReactNode;
  chatSlot?: ReactNode;
  children?: ReactNode;
  onMenu?: () => void;
  onChat?: () => void;
  onTab?: (id: string) => void;
  onExpandMap?: () => void;
  onToggleMap?: () => void;
  onHideHud?: () => void;
  onShowHud?: () => void;
  onActionPress?: (id: string) => void;
  onActionRelease?: (id: string) => void;
};

export default function MobileHudFrame({
  orientation = "landscape",
  place = "",
  coords = "",
  heading,
  mapSrc,
  mapCollapsed = false,
  unread = 0,
  crosshair = false,
  hudHidden = false,
  tabs,
  activeTab,
  actions,
  mainAction,
  actionsHidden = false,
  controlsSlot,
  chatSlot,
  children,
  onMenu,
  onChat,
  onTab,
  onExpandMap,
  onToggleMap,
  onHideHud,
  onShowHud,
  onActionPress,
  onActionRelease,
}: MobileHudFrameProps) {
  return (
    <>
      {controlsSlot ? (
        <div className="mhf__controls" data-orientation={orientation}>
          {controlsSlot}
        </div>
      ) : null}
      <div className="mhf" data-orientation={orientation} data-hud={hudHidden ? "hidden" : "shown"}>
        {hudHidden ? (
          <button type="button" className="mhf__showui" aria-label="Show interface" onClick={onShowHud}>
            <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
              <path d="M2.5 12C4 8.4 7.6 6 12 6s8 2.4 9.5 6c-1.5 3.6-5.1 6-9.5 6s-8-2.4-9.5-6Z"
                fill="none" stroke="currentColor" strokeWidth="1.9" />
              <circle cx="12" cy="12" r="2.8" fill="currentColor" />
            </svg>
          </button>
        ) : (
          <>
            {chatSlot ? <div className="mhf__chat">{chatSlot}</div> : null}

            <MobileTopBar
              orientation={orientation}
              place={place}
              coords={coords}
              unread={unread}
              onMenu={onMenu}
              onChat={onChat}
              onHideHud={onHideHud}
            />

            <MobileMinimapPuck
              orientation={orientation}
              place={place}
              coords={coords}
              heading={heading}
              mapSrc={mapSrc}
              collapsed={mapCollapsed}
              onExpand={onExpandMap}
              onToggleCollapsed={onToggleMap}
            />

            <MobileActionCluster
              orientation={orientation}
              actions={actions}
              mainAction={mainAction}
              hidden={actionsHidden}
              onActionPress={onActionPress}
              onActionRelease={onActionRelease}
            />

            <MobileTabBar orientation={orientation} tabs={tabs} active={activeTab} onTab={onTab} />

            {crosshair ? <span className="mhf__crosshair" aria-hidden="true" /> : null}
          </>
        )}

        {children}
      </div>
    </>
  );
}
