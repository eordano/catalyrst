import { Network } from "@dcl/schemas";
import { NameRegisteredEventArgs } from "../../abi/DCLRegistrar";
import { getAddresses } from "../../common/utils/addresses";
import {
  Account,
  AnalyticsDayData,
  Category,
  ENS,
  NFT,
  Network as ModelNetwork,
} from "../../model";
import { getNFTId } from "../../common/utils";
import {
  createAccount,
  createOrLoadAccount,
} from "../../common/modules/account";
import { NameBoughtEventArgs } from "../../abi/DCLControllerV2";
import { getOrCreateAnalyticsDayData } from "../../common/modules/analytics";

export function handleNameRegistered(
  event: NameRegisteredEventArgs,
  ensMap: Map<string, ENS>,
  nfts: Map<string, NFT>,
  accounts: Map<string, Account>,
  orderHash?: string
): void {
  const { _labelHash, _caller, _subdomain, _createdDate, _beneficiary } = event;
  const tokenId = BigInt(_labelHash);

  const addresses = getAddresses(Network.ETHEREUM);
  const id = getNFTId(addresses.DCLRegistrar, tokenId.toString(), Category.ens);

  const ens = new ENS({ id });
  ens.tokenId = BigInt(tokenId);
  // The owner is the beneficiary the name is registered *for*, never the caller
  // that registered it: a DCLControllerV2 purchase is called by the controller
  // contract, so keying off _caller records that contract as the owner of every
  // name bought through it. This entity also replaces the one buildENSFromNFT
  // derives from the mint, so its owner must agree with nft.owner.
  let owner = accounts.get(`${_beneficiary}-${ModelNetwork.ETHEREUM}`);
  if (!owner) {
    owner = createAccount(_beneficiary);
    accounts.set(`${_beneficiary}-${ModelNetwork.ETHEREUM}`, owner);
  }
  ens.owner = owner;
  ens.caller = _caller;
  ens.beneficiary = _beneficiary;
  ens.labelHash = _labelHash;
  ens.subdomain = _subdomain;
  ens.createdAt = _createdDate;

  // Store orderHash if provided (from Squid Router CORAL flow)
  if (orderHash) {
    ens.orderHash = orderHash;
  }

  ensMap.set(id, ens);

  const nft = nfts.get(id) || new NFT({ id });
  nft.name = ens.subdomain;
  nft.network = ModelNetwork.ETHEREUM;
  nft.searchText = ens.subdomain.toLowerCase();
  nfts.set(id, nft);

  createOrLoadAccount(accounts, _caller);
}

export function handleNameBought(
  event: NameBoughtEventArgs,
  timestamp: bigint,
  analytics: Map<string, AnalyticsDayData>
): void {
  const { _price } = event;
  const analyticsDayData = getOrCreateAnalyticsDayData(timestamp, analytics);

  analyticsDayData.daoEarnings = analyticsDayData.daoEarnings + _price;

  analytics.set(analyticsDayData.id, analyticsDayData);
}
