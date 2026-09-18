import { describe, expect, it } from "vitest";
import type { LinkedPublicationPreparation } from "@ui/generated/catalyst/builder/LinkedPublicationPreparation";
import { foundationPublication, FOUNDATION_BUILDER } from "./foundation-publication";

const id="11111111-1111-4111-8111-111111111111", itemId="22222222-2222-4222-8222-222222222222";
const owner="0x"+"11".repeat(20), provider="urn:decentraland:matic:collections-thirdparty:provider";
const p:LinkedPublicationPreparation={id,name:"Linked hats",creator:owner,urn:`${provider}:hats`,third_party_id:provider,item_ids:[itemId],revision:"reviewed"};
const data={category:"hat",tags:[],hides:[],replaces:[],representations:[{bodyShapes:["urn:decentraland:off-chain:base-avatars:BaseMale"],mainFile:"hat.glb",contents:["hat.glb"],overrideHides:[],overrideReplaces:[]}]};
function fixture() {
  const local={id:itemId,name:"Hat",description:"A hat",eth_address:owner,collection_id:id,type:"wearable",rarity:null,price:null,beneficiary:null,
    thumbnail:"thumbnail.png",data,metrics:{triangles:10},contents:{"hat.glb":"model-cid","thumbnail.png":"image-cid"},is_published:false};
  const state={collection:null as Record<string,unknown>|null,item:null as Record<string,unknown>|null,failFiles:false,tamper:false,missingHash:false,extraItem:false};
  const writes:{path:string;method:string;body:any}[]=[];
  const api=foundationPublication(()=>({base:"https://local.test",fetch:async(url,init={})=>{
    const method=init.method??"GET", body=typeof init.body==="string"?JSON.parse(init.body):init.body;
    const reply=(data:unknown)=>Response.json({data});
    if(url===`https://local.test/v1/items/${itemId}`) return reply(local);
    if(url===`https://local.test/v1/storage/contents/model-cid`) return new Response("model bytes");
    if(url===`https://local.test/v1/storage/contents/image-cid`) return new Response("thumbnail bytes");
    const path=url.slice(FOUNDATION_BUILDER.length);
    if(method!=="GET") writes.push({path,method,body});
    if(path===`/collections/${id}`) {
      if(method==="PUT") state.collection={...body.collection};
      return state.collection?reply(state.collection):Response.json({error:"Not found"},{status:404});
    }
    if(path===`/items/${itemId}`) { state.item=body.item; return reply(state.item); }
    if(path===`/items/${itemId}/files`) return state.failFiles?Response.json({error:"upload offline"},{status:503}):Response.json({ok:true});
    if(path===`/collections/${id}/items`) {
      const item={...state.item,local_content_hash:state.missingHash?null:"entity-hash",
        contents:state.tamper?{"hat.glb":"different"}:local.contents,
        metrics:{...local.metrics},data:{representations:data.representations,replaces:[],hides:[],tags:[],category:"hat"}};
      return reply(state.extraItem?[item,{...item,id:"33333333-3333-4333-8333-333333333333"}]:[item]);
    }
    if(path===`/collections/${id}/tos`) return Response.json({ok:true});
    if(path===`/collections/${id}/publish`) return reply({collection:state.collection,items:[state.item],itemCurations:[]});
    throw new Error(`Unexpected request ${method} ${url}`);
  }}));
  return {api,local,state,writes};
}

describe("Foundation linked publication",()=>{
  it("synchronizes linked URNs and original bytes, then records terms against Foundation's content hashes",async()=>{
    const f=fixture();await f.api.synchronizeLinked(p," creator@example.org ");
    expect(f.state.collection).toMatchObject({id,urn:p.urn,salt:null,contract_address:null});
    expect(f.state.item).toMatchObject({id:itemId,urn:`${p.urn}:${itemId}`,rarity:null,price:"0",metrics:{triangles:10},data});
    const upload=f.writes.find(w=>w.path.endsWith("/files"))!.body as FormData;
    expect([...(upload.keys())]).toEqual(["model-cid","image-cid"]);
    expect(await (upload.get("model-cid") as Blob).text()).toBe("model bytes");
    expect(f.writes.at(-1)).toMatchObject({path:`/collections/${id}/tos`,body:{email:"creator@example.org",event:"publish_third_party_items_tos",hashes:["entity-hash"]}});
    expect(f.writes.some(w=>w.path.endsWith("/publish"))).toBe(false);
  });
  it("allows a retry of an unpublished draft without changing its item identity",async()=>{
    const f=fixture();await f.api.synchronizeLinked(p,"a@b.org");const first=structuredClone(f.state.item);
    await f.api.synchronizeLinked(p,"a@b.org");expect(f.state.item).toEqual(first);
  });
  it("refuses submitted or mismatched upstream collections before writing",async()=>{
    for(const override of [{is_published:true},{urn:`${provider}:other`},{eth_address:"0x"+"22".repeat(20)}]) {
      const f=fixture();f.state.collection={id,name:p.name,eth_address:owner,urn:p.urn,contract_address:null,...override};
      await expect(f.api.synchronizeLinked(p,"a@b.org")).rejects.toThrow();expect(f.writes).toHaveLength(0);
    }
  });
  it("rejects invalid terms and duplicate or malformed item IDs before upstream mutations",async()=>{
    const f=fixture();await expect(f.api.synchronizeLinked(p,"bad email")).rejects.toThrow("email");
    for(const override of [{item_ids:[]},{item_ids:[itemId,itemId]},{item_ids:["../items"]},{urn:`${provider}:`},{third_party_id:"wrong"}]) {
      await expect(f.api.synchronizeLinked({...p,...override},"a@b.org")).rejects.toThrow();
    }
    expect(f.writes).toHaveLength(0);
  });
  it("does not proceed to terms after failed files, changed content, extra items or missing hashes",async()=>{
    for(const field of ["failFiles","tamper","extraItem","missingHash"] as const) {
      const f=fixture();f.state[field]=true;
      await expect(f.api.synchronizeLinked(p,"a@b.org")).rejects.toThrow();
      expect(f.writes.some(w=>w.path.endsWith("/tos")||w.path.endsWith("/publish"))).toBe(false);
    }
  });
  it("posts the exact saved cheque and item IDs without treating the response as proof of publication",async()=>{
    const f=fixture(),cheque={qty:1,salt:"0x"+"ab".repeat(32),signature:"0x"+"11".repeat(65)};
    expect(await f.api.submitLinked(p,cheque)).toBeUndefined();
    expect(f.writes).toEqual([{path:`/collections/${id}/publish`,method:"POST",body:{itemIds:[itemId],cheque}}]);
    await expect(f.api.submitLinked(p,{...cheque,qty:2})).rejects.toThrow("authorization");
    expect(f.writes).toHaveLength(1);
  });
});
