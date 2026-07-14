import type { ComponentType, ReactNode } from "react";

import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "../components/CreatorHubBreadcrumb";
import ChCollectionDetail, {
  type ChCollection,
  type ChCollectionItem,
} from "../pages/ChCollectionDetail";
import "../pages/chcollectiondetail.css";
import "./collectiondetailview.css";

type BreadcrumbLinkComponentProps = {
  className?: string;
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  children?: ReactNode;
};

type CollectionDetailViewProps = {
  signedIn?: boolean;
  chrome?: boolean;
  account?: string;
  name?: string;
  onSignIn?: () => void;
  loading?: boolean;
  collection?: ChCollection;
  wearables?: ChCollectionItem[];
  emotes?: ChCollectionItem[];
  initialItemType?: "wearable" | "emote";
  onBack?: () => void;
  onItemTypeChange?: (type: "wearable" | "emote") => void;
  onItemOpen?: (item: ChCollectionItem) => void;
  onPublish?: () => void;
  onAddItems?: () => void;
  LinkComponent?: ComponentType<BreadcrumbLinkComponentProps>;
};

export default function CollectionDetailView({
  signedIn = false,
  account = "",
  name = "",
  onSignIn = undefined,
  loading = false,
  collection = undefined,
  wearables = undefined,
  emotes = undefined,
  initialItemType = "wearable",
  onBack = undefined,
  onItemTypeChange = undefined,
  onItemOpen = undefined,
  onPublish = undefined,
  onAddItems = undefined,
  LinkComponent = undefined,
  chrome = true,
}: CollectionDetailViewProps) {
  return (
    <CreatorHubChromeMaybe
      chrome={chrome}
      active="collections"
      signedIn={signedIn}
      account={account}
      name={name}
      onSignIn={onSignIn}
    >
      <div className="creator-wearable-collection-detail-route">
        <CreatorHubBreadcrumb
          to="/create/wearables"
          label="Collections"
          LinkComponent={LinkComponent}
        />
        <ChCollectionDetail
          bare
          loading={loading}
          collection={collection}
          wearables={wearables}
          emotes={emotes}
          initialItemType={initialItemType}
          onBack={onBack}
          onItemTypeChange={onItemTypeChange}
          onItemOpen={onItemOpen}
          onPublish={onPublish}
          onAddItems={onAddItems}
        />
      </div>
    </CreatorHubChromeMaybe>
  );
}
