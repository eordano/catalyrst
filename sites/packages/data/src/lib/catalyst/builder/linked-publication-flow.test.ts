import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedPublicationPreparation } from "@ui/generated/catalyst/builder/LinkedPublicationPreparation";
import type { LinkedPublicationState } from "@ui/generated/catalyst/builder/LinkedPublicationState";
import { linkedPublicationFlow } from "./linked-publication-flow";

const mocks=vi.hoisted(()=>({inspect:vi.fn(),synchronizeLinked:vi.fn(),submitLinked:vi.fn(),authorize:vi.fn()}));
vi.mock("./foundation-publication",()=>({foundationPublication:()=>mocks}));
vi.mock("./linked-publish-wallet",()=>({linkedPublishWallet:()=>mocks.authorize}));
const owner="0x"+"11".repeat(20),id="11111111-1111-4111-8111-111111111111",item="22222222-2222-4222-8222-222222222222";
const provider="urn:decentraland:matic:collections-thirdparty:provider",salt="0x"+"ab".repeat(32);
const preparation:LinkedPublicationPreparation={id,revision:"reviewed",name:"Hats",creator:owner,urn:`${provider}:hats`,third_party_id:provider,item_ids:[item]};
const cheque={qty:1,salt,signature:"0x"+"11".repeat(65)};
function fixture() {
  const state={publication:null as LinkedPublicationState|null,address:owner,curated:false,verifyOffline:false,loseAuthorization:false,begin:0,submits:0,slots:3};
  const local=vi.fn(async(path:string,init:RequestInit={})=>{
    const method=init.method??"GET",body=init.body?JSON.parse(String(init.body)):undefined;
    const reply=(data:unknown)=>Response.json({data});
    if(path.endsWith("/status")) return reply(state.publication);
    if(path.endsWith("/verify")) {
      expect(method).toBe("POST");
      expect(body).toEqual({collection:{signed:`GET:/v1/collections/${id}`},items:{signed:`GET:/v1/collections/${id}/items`},curations:{signed:`GET:/v1/collections/${id}/itemCurations`}});
      if(state.verifyOffline) return Response.json({error:"Verification unavailable"},{status:503});
      if(state.curated) state.publication={...state.publication!,status:"submitted",forum_url:"https://forum.decentraland.org/t/hats/123"};
      return reply(state.publication);
    }
    if(method==="GET") return reply(preparation);
    if(method==="POST") {expect(body.revision).toBe(preparation.revision);state.begin++;state.publication={preparation,salt,status:"prepared",cheque:null,forum_url:null};}
    if(method==="PATCH") state.publication!.status="signing";
    if(method==="PUT") {
      if(state.loseAuthorization) return Response.json({error:"Could not save authorization"},{status:503});
      expect(body).toEqual(cheque);state.publication!.status="authorized";state.publication!.cheque=body;
    }
    if(method==="DELETE") state.publication=null;
    return reply(state.publication);
  });
  vi.stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL)=>{
    if(String(input).endsWith("/thirdParties")) return Response.json({data:[{id:provider,name:"Provider",managers:[owner],published:true}]});
    expect(String(input)).toBe(`/api/builder-foundation/thirdParties/${encodeURIComponent(provider)}/slots`);
    return Response.json({data:state.slots});
  }));
  mocks.authorize.mockResolvedValue(cheque);
  mocks.submitLinked.mockImplementation(async()=>{state.submits++;state.curated=true;});
  const create=()=>linkedPublicationFlow(()=>({address:state.address,fetch:local,provider:{request:async()=>{throw new Error("Unexpected RPC");}},sign:async(method,path)=>({headers:{signed:`${method}:${path}`}})}));
  return {state,local,create};
}
beforeEach(()=>{vi.resetAllMocks();});
afterEach(()=>{vi.unstubAllGlobals();});

describe("linked publication orchestration",()=>{
  it("inspects and reviews slots, synchronizes before signing, persists authorization before submission, and requires server confirmation",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);
    expect(review).toMatchObject({availableSlots:3,providerName:"Provider",preparation});
    mocks.authorize.mockImplementation(async intent=>{
      expect(intent).toEqual({thirdPartyId:provider,qty:1,salt});expect(f.state.publication?.status).toBe("signing");
      expect(mocks.synchronizeLinked).toHaveBeenCalledOnce();return cheque;
    });
    mocks.submitLinked.mockImplementation(async()=>{
      expect(f.state.publication?.cheque).toEqual(cheque);f.state.curated=true;
    });
    expect((await flow.publish({id,review,email:"a@b.org"})).status).toBe("submitted");
    expect(mocks.inspect).toHaveBeenCalledOnce();expect(mocks.authorize).toHaveBeenCalledOnce();
    expect(mocks.submitLinked).toHaveBeenCalledWith(preparation,cheque,undefined);
  });
  it("recovers a lost publish response through authoritative review without signing or submitting twice",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);
    mocks.submitLinked.mockImplementation(async()=>{f.state.submits++;f.state.curated=true;f.state.verifyOffline=true;throw new Error("Publish response lost");});
    await expect(flow.publish({id,review,email:"a@b.org"})).rejects.toThrow("Verification unavailable");
    expect(f.state.publication?.status).toBe("authorized");
    f.state.verifyOffline=false;
    const reopened=f.create();expect((await reopened.review(id)).availableSlots).toBeNull();
    expect((await reopened.publish({id,email:""})).status).toBe("submitted");
    expect(mocks.authorize).toHaveBeenCalledOnce();expect(f.state.submits).toBe(1);expect(f.state.begin).toBe(1);
  });
  it("continues with the same saved cheque when Foundation has not received the first attempt",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);
    mocks.submitLinked.mockRejectedValueOnce(new Error("Foundation offline"));
    await expect(flow.publish({id,review,email:"a@b.org"})).rejects.toThrow("Foundation offline");
    expect((await f.create().publish({id,email:""})).status).toBe("submitted");
    expect(mocks.authorize).toHaveBeenCalledOnce();expect(mocks.synchronizeLinked).toHaveBeenCalledOnce();
    expect(mocks.submitLinked.mock.calls.map(call=>call[1])).toEqual([cheque,cheque]);
  });
  it("does not submit before saving authorization and reuses the claimed nonce after a save failure",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);f.state.loseAuthorization=true;
    await expect(flow.publish({id,review,email:"a@b.org"})).rejects.toThrow("Could not save authorization");
    expect(mocks.submitLinked).not.toHaveBeenCalled();expect(f.state.publication?.status).toBe("signing");
    f.state.loseAuthorization=false;await f.create().publish({id,email:"a@b.org"});
    expect(mocks.authorize.mock.calls.map(call=>call[0].salt)).toEqual([salt,salt]);expect(f.state.begin).toBe(1);
  });
  it("never treats an upstream publish response alone as success",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);mocks.submitLinked.mockResolvedValue({is_published:true});
    await expect(flow.publish({id,review,email:"a@b.org"})).rejects.toThrow("has not confirmed");
    expect(f.state.publication?.status).toBe("authorized");
  });
  it("halts on account changes during synchronization",async()=>{
    const f=fixture(),flow=f.create();const review=await flow.review(id);
    mocks.synchronizeLinked.mockImplementation(async()=>{f.state.address="0x"+"22".repeat(20);});
    await expect(flow.publish({id,review,email:"a@b.org"})).rejects.toThrow("account changed");
    expect(mocks.authorize).not.toHaveBeenCalled();expect(mocks.submitLinked).not.toHaveBeenCalled();
  });
});
