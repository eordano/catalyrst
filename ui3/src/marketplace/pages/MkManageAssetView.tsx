import type { ComponentProps } from "react";

import AssetActionLayout from "../frames/AssetActionLayout";
import MkManageAssetPage from "./MkManageAssetPage";
import "./manageassetview.css";

type MkManageAssetPageProps = ComponentProps<typeof MkManageAssetPage>;

export type MkManageAction = "sell" | "transfer" | "cancel";

export type MkManageAssetViewProps = {
  actionHub?: boolean;
  name?: string;
  image?: string | null;
  rarity?: string;
  listed?: boolean;
  locked?: boolean;
  pending?: MkManageAction | null;
  orderPrice?: string;
  orderExpiresAt?: string;
  pageAsset?: MkManageAssetPageProps["asset"];
  pageOrder?: MkManageAssetPageProps["order"];
  onAction?: (action: MkManageAction) => void;
};

export default function MkManageAssetView({
  actionHub = false,
  name = "",
  image = null,
  rarity = "",
  listed = false,
  locked = false,
  pending = null,
  orderPrice = undefined,
  orderExpiresAt = undefined,
  pageAsset = undefined,
  pageOrder = undefined,
  onAction = undefined,
}: MkManageAssetViewProps) {
  return (
    <div className="mkmanage-route">
      {actionHub && (
        <div className="mkmanage-route__hub" data-variant="treatment">
          <AssetActionLayout
            theme="dark"
            hideBack
            title={name}
            subtitle={
              listed
                ? `Listed for ◇ ${orderPrice} MANA · expires ${orderExpiresAt}`
                : "Not listed — choose an action below."
            }
            warning={null}
            icon={null}
            media={
              <div className="mkmanage-route__media">
                {image ? (
                  <img src={image} alt={name} />
                ) : (
                  <div className="mkmanage-route__mediaph" aria-hidden="true" />
                )}
                <span className="mkmanage-route__rarity">{rarity}</span>
              </div>
            }
          >
            <div className="mkmanage-route__actions" role="group" aria-label="Asset actions">
              {listed ? (
                <button
                  type="button"
                  className="mkmanage-route__action mkmanage-route__action--danger"
                  disabled={locked || pending === "cancel"}
                  onClick={() => onAction?.("cancel")}
                >
                  Cancel listing
                </button>
              ) : (
                <button
                  type="button"
                  className="mkmanage-route__action mkmanage-route__action--primary"
                  disabled={locked || pending === "sell"}
                  onClick={() => onAction?.("sell")}
                >
                  Sell
                </button>
              )}
              <button
                type="button"
                className="mkmanage-route__action"
                disabled={locked || pending === "transfer"}
                onClick={() => onAction?.("transfer")}
              >
                Transfer
              </button>
            </div>
            {locked && (
              <p className="mkmanage-route__locked" role="status">
                This asset has an active rental, so sell and transfer are locked
                until it ends.
              </p>
            )}
          </AssetActionLayout>
        </div>
      )}

      <MkManageAssetPage
        asset={pageAsset}
        order={pageOrder}
        rental={undefined}
        locked={locked}
      />

      {!actionHub && (
        <div className="mkmanage-route__controlactions">
          {listed ? (
            <button
              type="button"
              className="mkmanage-route__action mkmanage-route__action--danger"
              disabled={locked}
              onClick={() => onAction?.("cancel")}
            >
              Cancel listing
            </button>
          ) : (
            <button
              type="button"
              className="mkmanage-route__action mkmanage-route__action--primary"
              disabled={locked}
              onClick={() => onAction?.("sell")}
            >
              Sell
            </button>
          )}
          <button
            type="button"
            className="mkmanage-route__action"
            disabled={locked}
            onClick={() => onAction?.("transfer")}
          >
            Transfer
          </button>
        </div>
      )}
    </div>
  );
}
