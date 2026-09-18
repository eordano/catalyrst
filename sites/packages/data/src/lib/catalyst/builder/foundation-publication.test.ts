import { describe, expect, it, vi } from "vitest";
import { encodeFunctionResult, keccak256, parseAbi, stringToHex } from "viem";
import type { PublicationPreparation } from "@ui/generated/catalyst/builder/PublicationPreparation";
import type { Eip1193Provider } from "../../auth/wallet";
import { COLLECTION_BASE_URI, COLLECTION_FACTORY, COLLECTION_FORWARDER, COLLECTION_MANAGER, type PublicationQuote } from "./collection-publication";
import type { measureItemModels } from "./model-metrics";
import { FOUNDATION_BUILDER, foundationPublication } from "./foundation-publication";

const owner="0x1111111111111111111111111111111111111111";
const contract="0x2222222222222222222222222222222222222222";
const id="11111111-1111-4111-8111-111111111111";
const itemId="22222222-2222-4222-8222-222222222222";
const metrics={triangles:1,meshes:1,bodies:1,materials:1,textures:0,entities:1};
function fixture() {
  const preparation:PublicationPreparation={id,revision:"reviewed",chain_id:137,manager:COLLECTION_MANAGER,factory:COLLECTION_FACTORY,forwarder:COLLECTION_FORWARDER,
    salt:keccak256(stringToHex(id)),name:"My collection",symbol:"DCL-CLLCTN",base_uri:COLLECTION_BASE_URI,creator:owner,
    items:[{id:itemId,rarity:"rare",price:"0",beneficiary:"0x0000000000000000000000000000000000000000",metadata:"1:w:Hat::hat:BaseMale"}]};
  const quote:PublicationQuote={preparation,blockNumber:"1",rarities:owner,totalWei:"1",totalMana:"0.000000000000000001",lines:[],transaction:{from:owner,to:COLLECTION_MANAGER,data:"0x"}};
  const item={id:itemId,name:"Hat",description:"A hat",type:"wearable",eth_address:owner,collection_id:id,rarity:"rare",price:"0",beneficiary:null,
    thumbnail:"thumbnail.png" as string|null,data:{category:"hat",representations:[{bodyShapes:["urn:decentraland:off-chain:base-avatars:BaseMale"],mainFile:"hat.gltf",contents:["hat.gltf"]}]},
    metrics:null as Record<string,number>|null,contents:{"hat.gltf":"model-hash","thumbnail.png":"image-hash"},is_published:false,blockchain_item_id:"0",created_at:1};
  const collection={id,name:preparation.name,eth_address:owner,salt:preparation.salt,contract_address:contract,is_published:false,forum_link:null as string|null};
  const state={indexed:true,losePost:false,reordered:false,badAddress:false,failFiles:false,posts:0};
  const requests:{url:string;method:string;body:unknown}[]=[];
  const fetchImpl:typeof fetch=async (input,init={})=>{
    const url=String(input),method=init.method??"GET";
    const body=typeof init.body==="string"?JSON.parse(init.body):init.body;
    requests.push({url,method,body});
    const reply=(data:unknown)=>Response.json({data});
    if (url.startsWith("https://local.test/v1/storage/contents/")) return new Response("model bytes");
    if (url===`https://local.test/v1/collections/${id}/items`) return reply([{id:itemId}]);
    if (url===`https://local.test/v1/items/${itemId}`) {
      if (method==="PUT") Object.assign(item,body.item);
      return reply(item);
    }
    if (url===`https://local.test/v1/items/${itemId}/files`) return reply({"thumbnail.png":"generated-image"});
    if (url===`${FOUNDATION_BUILDER}/collections/${id}`) return reply({...collection,contract_address:state.badAddress?owner:contract});
    if (url===`${FOUNDATION_BUILDER}/items/${itemId}`) return reply(body.item);
    if (url===`${FOUNDATION_BUILDER}/items/${itemId}/files`) return state.failFiles?Response.json({error:"storage offline"},{status:503}):Response.json({ok:true});
    if (url===`${FOUNDATION_BUILDER}/collections/${id}/items`) return reply(state.reordered?[]:[item]);
    if (url===`${FOUNDATION_BUILDER}/collections/${id}/tos`) return Response.json({ok:true});
    if (url===`${FOUNDATION_BUILDER}/collections/${id}/publish`) return reply({collection:{...collection,is_published:state.indexed},items:[{...item,blockchain_item_id:state.indexed?"0":null}]});
    if (url===`${FOUNDATION_BUILDER}/collections/${id}/post`) {
      state.posts++;collection.forum_link="https://forum.decentraland.org/t/collection/123";
      if (state.losePost) throw new Error("response lost");
      return reply(collection.forum_link);
    }
    throw new Error(`Unexpected ${method} ${url}`);
  };
  const measure=vi.fn<typeof measureItemModels>(async()=>metrics);
  const api=foundationPublication(()=>({fetch:fetchImpl,base:"https://local.test"}),{measure});
  const provider:Eip1193Provider={async request({method}){
    if(method==="eth_call") return encodeFunctionResult({abi:parseAbi(["function getAddress(bytes32,address,bytes) view returns (address)"]),functionName:"getAddress",result:contract});
    throw new Error(`Unexpected RPC ${method}`);
  }};
  return {api,quote,preparation,item,collection,state,requests,provider,measure};
}

describe("Foundation collection publication",()=>{
  it("inspects stored bytes and persists measured metrics plus strict Foundation metadata",async()=>{
    const f=fixture();await f.api.inspect(id);
    expect(f.item.metrics).toEqual(metrics);
    expect(f.item.data).toMatchObject({tags:[],hides:[],replaces:[],representations:[{overrideHides:[],overrideReplaces:[]}]});
    expect(f.measure).toHaveBeenCalledOnce();
    expect(f.measure.mock.calls[0]![0]).toBe("wearable");
    expect(f.requests.some(r=>r.url==="https://local.test/v1/storage/contents/model-hash")).toBe(true);
    f.requests.length=0;await f.api.inspect(id);
    expect(f.requests.filter(r=>r.method==="PUT")).toHaveLength(0);
  });
  it("requires an emote thumbnail and never fabricates metrics on inspection failure",async()=>{
    const f=fixture();f.item.type="emote";f.item.thumbnail=null;Object.assign(f.item.data,{loop:true});
    await expect(f.api.inspect(id)).rejects.toThrow("thumbnail.png");
    expect(f.requests.filter(r=>r.method==="PUT")).toHaveLength(0);
    f.item.type="wearable";f.measure.mockRejectedValueOnce(new Error("malformed geometry"));
    await expect(f.api.inspect(id)).rejects.toThrow("malformed geometry");
    expect(f.item.metrics).toBeNull();
  });
  it("uploads original content by hash and records TOS only after verifying item order",async()=>{
    const f=fixture();await f.api.inspect(id);await f.api.synchronize(f.quote,"creator@example.org",f.provider);
    const upload=f.requests.find(r=>r.url.endsWith(`/items/${itemId}/files`)&&r.url.startsWith(FOUNDATION_BUILDER));
    expect([...(upload!.body as FormData).keys()]).toEqual(["model-hash","image-hash"]);
    expect(f.requests.at(-1)).toMatchObject({url:`${FOUNDATION_BUILDER}/collections/${id}/tos`,body:{email:"creator@example.org",event:"publish_collection_tos",collection_address:contract}});
    const remoteItem=f.requests.find(r=>r.url===`${FOUNDATION_BUILDER}/items/${itemId}`)!.body as {item:Record<string,unknown>};
    expect(remoteItem.item).toMatchObject({metrics,beneficiary:f.preparation.items[0].beneficiary});
  });
  it("rejects invalid email/salt/address/order and failed uploads before proceeding",async()=>{
    const f=fixture();await f.api.inspect(id);f.requests.length=0;
    await expect(f.api.synchronize(f.quote,"invalid",f.provider)).rejects.toThrow("email");
    expect(f.requests).toHaveLength(0);
    const salt=f.preparation.salt;f.preparation.salt=`0x${"ff".repeat(32)}`;
    await expect(f.api.synchronize(f.quote,"a@b.org",f.provider)).rejects.toThrow("salt");f.preparation.salt=salt;
    f.state.badAddress=true;await expect(f.api.synchronize(f.quote,"a@b.org",f.provider)).rejects.toThrow("does not match");f.state.badAddress=false;
    f.state.reordered=true;await expect(f.api.synchronize(f.quote,"a@b.org",f.provider)).rejects.toThrow("item order");f.state.reordered=false;
    f.state.failFiles=true;await expect(f.api.synchronize(f.quote,"a@b.org",f.provider)).rejects.toThrow("storage offline");
    expect(f.requests.some(r=>r.url.endsWith("/tos"))).toBe(false);
  });
  it("waits for indexed item IDs and retries review submission without another deployment",async()=>{
    const f=fixture();f.state.indexed=false;
    await expect(f.api.submit(f.preparation,contract)).rejects.toThrow("still indexing");expect(f.state.posts).toBe(0);
    f.state.indexed=true;expect(await f.api.submit(f.preparation,contract)).toEqual({forumUrl:"https://forum.decentraland.org/t/collection/123",curationSubmitted:true});
    const post=f.requests.find(r=>r.url.endsWith("/post"))!.body as {forumPost:{raw:string}};
    expect(post.forumPost.raw).toContain(`item-editor?collection=${id}`);
    expect(post.forumPost.raw).toContain(`${FOUNDATION_BUILDER}/storage/contents/image-hash`);
    await f.api.submit(f.preparation,contract);expect(f.state.posts).toBe(1);
    expect(f.requests.some(r=>r.method==="PUT"||r.url.endsWith("/tos"))).toBe(false);
  });
  it("recovers a lost forum response and refuses unsafe forum links",async()=>{
    const f=fixture();f.state.losePost=true;
    await expect(f.api.submit(f.preparation,contract)).resolves.toMatchObject({curationSubmitted:true});
    await f.api.submit(f.preparation,contract);expect(f.state.posts).toBe(1);
    f.collection.forum_link="https://other.example/t/stolen";
    await expect(f.api.submit(f.preparation,contract)).rejects.toThrow("invalid review topic");
  });
});
