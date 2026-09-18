import { describe, expect, it } from "vitest";
import { decodeFunctionData, encodeFunctionResult, erc20Abi, maxUint256, parseUnits, type Hex } from "viem";
import type { PublicationPreparation } from "@ui/generated/catalyst/builder/PublicationPreparation";
import type { PublicationState } from "@ui/generated/catalyst/builder/PublicationState";
import type { Eip1193Provider } from "../../auth/wallet";
import { collectionPublishWallet } from "./collection-publish-wallet";
import { COLLECTION_BASE_URI, COLLECTION_FACTORY, COLLECTION_FORWARDER, COLLECTION_MANAGER, COLLECTION_MANA, collectionManagerAbi, collectionRaritiesAbi } from "./collection-publication";

const owner = "0x1111111111111111111111111111111111111111";
const contract = "0x2222222222222222222222222222222222222222";
const oracle = "0x3333333333333333333333333333333333333333";
const preparation: PublicationPreparation = { id:"collection", revision:"saved-revision", chain_id:137, manager:COLLECTION_MANAGER,
  factory:COLLECTION_FACTORY, forwarder:COLLECTION_FORWARDER, salt:`0x${"ab".repeat(32)}`, name:"Collection", symbol:"DCL-CLLCTN", base_uri:COLLECTION_BASE_URI,
  creator:owner, items:[{id:"item", rarity:"rare", price:"0", beneficiary:`0x${"00".repeat(20)}`, metadata:"1:w:Hat::hat:BaseMale"}] };
function fixture() {
  const state = {price:parseUnits("123.500000000000000001",18), allowance:maxUint256, balance:parseUnits("5000",18), account:owner,
    chain:"0x89", reject:false, pending:false, offline:false, reverted:false, approvalPending:false, lostResponse:false, cancels:0, publication:null as PublicationState|null};
  const storage = new Map<string,string>();
  const sent: {from:string; to:Hex; data:Hex}[] = [];
  const abi = [...collectionManagerAbi, ...collectionRaritiesAbi, ...erc20Abi];
  const provider: Eip1193Provider = { async request({method,params}) {
    if (method === "eth_accounts") return [state.account];
    if (method === "eth_chainId") return state.chain;
    if (method === "wallet_switchEthereumChain") { state.chain="0x89"; return null; }
    if (method === "eth_blockNumber") return "0x10";
    if (method === "eth_sendTransaction") {
      if (state.reject) throw Object.assign(new Error("User rejected transaction"), {code:4001});
      sent.push(params![0] as typeof sent[number]);
      if (state.lostResponse && sent.length === 2) throw new Error("Wallet response lost");
      return `0x${sent.length.toString(16).padStart(64,"0")}`;
    }
    if (method === "eth_getTransactionReceipt") {
      if (state.approvalPending) return null;
      const tx = sent[Number(BigInt(String(params![0])))-1];
      const decoded = decodeFunctionData({abi, data:tx.data});
      if (decoded.functionName === "approve") state.allowance=decoded.args[1];
      return {status:"0x1"};
    }
    if (method === "eth_call") {
      const tx = params![0] as {data:Hex};
      const decoded = decodeFunctionData({abi, data:tx.data});
      switch(decoded.functionName) {
        case "acceptedToken": return encodeFunctionResult({abi:collectionManagerAbi, functionName:"acceptedToken", result:COLLECTION_MANA});
        case "rarities": return encodeFunctionResult({abi:collectionManagerAbi, functionName:"rarities", result:oracle});
        case "getRarityByName": return encodeFunctionResult({abi:collectionRaritiesAbi, functionName:"getRarityByName", result:{name:"rare", maxSupply:5000n, price:state.price}});
        case "allowance": return encodeFunctionResult({abi:erc20Abi, functionName:"allowance", result:state.allowance});
        case "balanceOf": return encodeFunctionResult({abi:erc20Abi, functionName:"balanceOf", result:state.balance});
        case "approve": case "createCollection": return "0x";
      }
    }
    throw new Error(`Unexpected RPC ${method}`);
  }};
  const fetcher = async (path: string, init?: RequestInit) => {
    if (path.endsWith("/status")) return Response.json({data:state.publication});
    const body = init?.body ? JSON.parse(String(init.body)) : {};
    switch(init?.method ?? "GET") {
      case "GET": return Response.json({data:preparation});
      case "POST":
        expect(body.revision).toBe(preparation.revision);
        state.publication ??= {preparation,status:"prepared",tx_hash:null,contract_address:null};
        return Response.json({data:state.publication});
      case "PATCH":
        if (state.publication?.status !== "prepared") return Response.json({message:"Another wallet request is active"}, {status:409});
        state.publication.status="signing"; return Response.json({data:true});
      case "DELETE": state.cancels++; state.publication=null; return Response.json({data:true});
      case "PUT": {
        if (state.offline) return Response.json({message:"Builder offline"}, {status:503});
        const tx = sent[Number(BigInt(body.tx_hash))-1];
        expect(tx.to).toBe(COLLECTION_MANAGER);
        expect(decodeFunctionData({abi:collectionManagerAbi,data:tx.data}).functionName).toBe("createCollection");
        state.publication = {preparation,status:state.reverted?"reverted":state.pending?"submitted":"published",tx_hash:body.tx_hash,contract_address:state.pending||state.reverted?null:contract};
        return Response.json({data:state.publication});
      }
      default: throw new Error(`Unexpected HTTP ${init?.method}`);
    }
  };
  const adapter = () => collectionPublishWallet(() => ({address:owner,provider,fetch:fetcher}), {
    pollMs:1,timeoutMs:1,storage:{getItem:k=>storage.get(k)??null,setItem:(k,v)=>{storage.set(k,v);},removeItem:k=>{storage.delete(k);}},
  });
  return {state,sent,adapter,storage};
}

describe("collection wallet publication", () => {
  it("caps even an existing unlimited allowance to the exact reviewed fee and waits for backend verification", async () => {
    const {adapter,sent,state} = fixture(); const wallet=adapter();
    const quote=await wallet.quote({id:preparation.id});
    const result=await wallet.publish({id:preparation.id,quote});
    expect(result).toEqual({txHash:`0x${"2".padStart(64,"0")}`,contractAddress:contract,simulated:false,curationSubmitted:false});
    expect(sent).toHaveLength(2);
    expect(decodeFunctionData({abi:erc20Abi,data:sent[0].data})).toMatchObject({functionName:"approve",args:[expect.any(String),state.price]});
    expect(state.allowance).toBe(BigInt(quote.totalWei));
    await wallet.publish({id:preparation.id});
    expect(sent).toHaveLength(2);
  });
  it("finishes Foundation preparation before any payment and skips it when resuming a paid collection", async () => {
    const {adapter,sent,state}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    await expect(wallet.publish({id:preparation.id,quote,prepare:async()=>{
      expect(state.publication?.status).toBe("prepared");
      expect(sent).toHaveLength(0);
      throw new Error("Foundation unavailable");
    }})).rejects.toThrow("Foundation unavailable");
    expect(sent).toHaveLength(0);
    await wallet.publish({id:preparation.id,quote,prepare:async()=>{expect(sent).toHaveLength(0);}});
    await wallet.publish({id:preparation.id,prepare:async()=>{throw new Error("must not prepare again");}});
    expect(sent).toHaveLength(2);
  });
  it("releases the draft when the user rejects the wallet request", async () => {
    const {adapter,state,sent}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    state.reject=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("rejected");
    expect(state.cancels).toBe(1); expect(state.publication).toBeNull(); expect(sent).toHaveLength(0);
  });
  it("resumes the same approval after recreating the adapter", async () => {
    const {adapter,state,sent}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    state.approvalPending=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("approval is still pending");
    state.approvalPending=false;
    await adapter().publish({id:preparation.id,quote});
    expect(sent).toHaveLength(2);
  });
  it("retains a broadcast hash across backend failure and reload without requesting another payment", async () => {
    const {adapter,state,sent}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    state.offline=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("Builder offline");
    await expect(wallet.cancel(preparation.id)).rejects.toThrow("submitted");
    state.offline=false;
    await adapter().publish({id:preparation.id});
    expect(sent).toHaveLength(2);
  });
  it("resumes from server state even when browser storage is gone", async () => {
    const {adapter,state,sent,storage}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    state.pending=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("Polygon finality");
    storage.clear(); state.pending=false; state.chain="0x1";
    await adapter().publish({id:preparation.id});
    expect(sent).toHaveLength(2); expect(state.chain).toBe("0x1");
  });
  it("rejects fee changes and account changes before requesting a transaction", async () => {
    const {adapter,state,sent}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id});
    state.price++;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("fee changed");
    state.price--; state.account=contract;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("account changed");
    expect(sent).toHaveLength(0);
  });
  it("does not send twice when the wallet broadcasts but its response is lost", async () => {
    const {adapter,state,sent,storage}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id}); state.lostResponse=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("Wallet response lost");
    await expect(adapter().publish({id:preparation.id,quote})).rejects.toThrow("unresolved");
    await expect(adapter().cancel(preparation.id)).rejects.toThrow("submitted");
    expect(sent).toHaveLength(2);
    storage.clear();
    await expect(adapter().publish({id:preparation.id,quote})).rejects.toThrow("unresolved");
    const result=await adapter().recover({id:preparation.id,txHash:`0x${"2".padStart(64,"0")}`});
    expect(result.contractAddress).toBe(contract);
  });
  it("surfaces a verified revert and clears the saved publication hash", async () => {
    const {adapter,state,storage}=fixture(); const wallet=adapter(); const quote=await wallet.quote({id:preparation.id}); state.reverted=true;
    await expect(wallet.publish({id:preparation.id,quote})).rejects.toThrow("reverted");
    expect([...storage.keys()].some(key=>key.endsWith(":publish"))).toBe(false);
  });
});
