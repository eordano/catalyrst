import { z } from "zod";
import { createPublicClient, custom, encodeFunctionData, keccak256, parseAbi, stringToHex, type Address, type Hex } from "viem";
import type { LinkedPublicationPreparation } from "@ui/generated/catalyst/builder/LinkedPublicationPreparation";
import type { LinkedPublicationCheque } from "@ui/generated/catalyst/builder/LinkedPublicationCheque";
import { LinkedPublicationPreparationSchema } from "../generated-schemas/builder";
import type { PublicationPreparation } from "@ui/generated/catalyst/builder/PublicationPreparation";
import type { Eip1193Provider } from "../../auth/wallet";
import { CatalystError } from "../client";
import type { DraftOptions } from "./drafts";
import { linkedProviderId } from "./linked-providers";
import { measureItemModels } from "./model-metrics";
import { COLLECTION_FACTORY, COLLECTION_FORWARDER, type PublicationQuote } from "./collection-publication";

import { FOUNDATION_BUILDER } from "./foundation-fetch";
export { FOUNDATION_BUILDER } from "./foundation-fetch";
const Representation = z.object({bodyShapes:z.array(z.string()).min(1), mainFile:z.string(), contents:z.array(z.string()).min(1),
  overrideHides:z.array(z.string()).optional(), overrideReplaces:z.array(z.string()).optional()});
const Item = z.object({id:z.string().uuid(), name:z.string(), description:z.string().nullish().transform(v=>v??""), type:z.enum(["wearable","emote"]),
  eth_address:z.string(), collection_id:z.string(), rarity:z.string().nullable(), price:z.string().nullable(), beneficiary:z.string().nullable(), thumbnail:z.string().nullable(),
  data:z.record(z.string(),z.unknown()), metrics:z.record(z.string(),z.number()).nullish(), contents:z.record(z.string(),z.string()), is_published:z.boolean(),
  urn:z.string().nullish(), local_content_hash:z.string().nullish(), blockchain_item_id:z.string().nullish(), created_at:z.union([z.string(),z.number()]).optional()});
type Item = z.infer<typeof Item>;
const RemoteCollection = z.object({id:z.string(), name:z.string(), eth_address:z.string(), salt:z.string(), contract_address:z.string(),
  is_published:z.boolean().optional(), forum_link:z.string().nullish()});
const LinkedCollection = z.object({id:z.string().uuid(),name:z.string(),eth_address:z.string(),urn:z.string(),
  contract_address:z.null(),is_published:z.boolean().optional(),forum_link:z.string().nullish()});
function equalData(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (!a || !b || typeof a !== "object" || typeof b !== "object" || Array.isArray(a) !== Array.isArray(b)) return false;
  const entries = Object.entries(a), other = Object.entries(b);
  return entries.length === other.length && entries.every(([key,value]) => Object.hasOwn(b,key) && equalData(value,(b as Record<string,unknown>)[key]));
}
const initializationAbi = parseAbi(["function initialize(string name,string symbol,string baseURI,address creator,bool complete,bool approved,address rarities,(string rarity,uint256 price,address beneficiary,string metadata)[] items)"]);
const factoryAbi = parseAbi(["function getAddress(bytes32 salt,address sender,bytes data) view returns (address)"]);

async function request(path:string, opts:DraftOptions, init:RequestInit = {}):Promise<unknown> {
  const url=`${opts.base??""}${path}`;
  const response=await opts.fetch(url,{...init,signal:opts.signal,cache:"no-store"});
  const body=await response.json().catch(()=>null);
  if (!response.ok || body?.ok===false) throw new CatalystError(body?.message??body?.error??body?.data?.message??`Builder returned ${response.status}.`,url,response.status);
  if (!body || (!("data" in body) && body.ok!==true)) throw new Error("Builder returned an invalid response.");
  return body.data;
}

function normalizedData(item: Item) {
  const data = item.data;
  const representations = z.array(Representation).min(1).parse(data.representations).map(rep=>item.type === "wearable" ?
    {...rep, overrideHides:rep.overrideHides??[], overrideReplaces:rep.overrideReplaces??[]} : {bodyShapes:rep.bodyShapes,mainFile:rep.mainFile,contents:rep.contents});
  const result: Record<string,unknown> = {category:data.category,representations,tags:data.tags??[]};
  const optional = item.type === "wearable" ? ["removesDefaultHiding","requiredPermissions","blockVrmExport","outlineCompatible","springBones"] : ["startAnimation","outcomes","randomizeOutcomes"];
  if (item.type === "wearable") Object.assign(result,{hides:data.hides??[],replaces:data.replaces??[]});
  else result.loop = z.boolean().parse(data.loop);
  for (const field of optional) if (data[field] !== undefined && data[field] !== null) result[field]=data[field];
  return result;
}
function forumURL(raw: string): string {
  const url = new URL(raw);
  if (url.protocol !== "https:" || url.hostname !== "forum.decentraland.org" || !url.pathname.startsWith("/t/")) throw new Error("Foundation returned an invalid review topic URL.");
  return url.href;
}
function markdown(text: string) { return text.replace(/[\\`*_{}\[\]()#+.!<>]/g,"\\$&"); }

export function foundationPublication(options: () => DraftOptions, settings: {base?:string; measure?:typeof measureItemModels} = {}) {
  const local = (signal?:AbortSignal) => ({...options(),signal});
  const remote = (signal?:AbortSignal) => ({...local(signal),base:settings.base??FOUNDATION_BUILDER});
  const getItem = async (id:string,signal?:AbortSignal) => Item.parse(await request(`/v1/items/${encodeURIComponent(id)}`,local(signal)));
  async function contents(item: Item, signal?:AbortSignal) {
    const files = new Map<string,Blob>();
    let size = 0;
    for (const [name,hash] of Object.entries(item.contents)) {
      signal?.throwIfAborted();
      const opts=options();
      const response = await opts.fetch(`${opts.base??""}/v1/storage/contents/${encodeURIComponent(hash)}`, {signal,cache:"no-store"});
      if (!response.ok) throw new Error(`Could not load ${name} for publication.`);
      const blob = await response.blob(); size+=blob.size;
      if (size>20*1024*1024) throw new Error("An item exceeds the 20 MB publication limit.");
      files.set(name,blob);
    }
    return files;
  }
  async function uploadContents(item:Item,signal?:AbortSignal) {
    const form=new FormData(), files=await contents(item,signal), uploaded=new Set<string>();
    for (const [name,hash] of Object.entries(item.contents)) if (!uploaded.has(hash)) {
      form.append(hash,files.get(name)!,name); uploaded.add(hash);
    }
    await request(`/items/${item.id}/files`,remote(signal),{method:"POST",body:form});
  }
  async function remoteCollection(id:string,signal?:AbortSignal) {
    return RemoteCollection.parse(await request(`/collections/${encodeURIComponent(id)}`,remote(signal)));
  }
  function checkCollection(collection:z.infer<typeof RemoteCollection>, p:PublicationPreparation, expected?:string) {
    if (collection.id!==p.id || collection.eth_address.toLowerCase()!==p.creator.toLowerCase() || collection.salt.toLowerCase()!==p.salt.toLowerCase()
      || (expected && collection.contract_address.toLowerCase()!==expected.toLowerCase())) throw new Error("Foundation's collection does not match the reviewed deployment.");
  }
  return {
    async inspect(id:string,signal?:AbortSignal) {
      const rows = z.array(z.object({id:z.string()})).parse(await request(`/v1/collections/${encodeURIComponent(id)}/items`,local(signal)));
      if (!rows.length) throw new Error("Add an item to this collection before publishing.");
      for (const row of rows) {
        const item = await getItem(row.id,signal);
        if (item.is_published) continue;
        const data=normalizedData(item);
        const files=await contents(item,signal);
        let thumbnail: Blob|undefined;
        const metrics=await (settings.measure??measureItemModels)(item.type,z.array(Representation).parse(data.representations).map(rep=>rep.mainFile),files,signal,
          item.thumbnail?undefined:blob=>{thumbnail=blob;});
        const hashes={...item.contents};
        let thumbnailName=item.thumbnail;
        if (!thumbnailName) {
          if (!thumbnail) throw new Error(`${item.name}: include thumbnail.png in the emote ZIP before publishing.`);
          const form=new FormData(); form.append("files",thumbnail,"thumbnail.png");
          Object.assign(hashes,z.record(z.string(),z.string()).parse(await request(`/v1/items/${item.id}/files`,local(signal),{method:"POST",body:form})));
          thumbnailName="thumbnail.png";
        }
        if (JSON.stringify(item.data)!==JSON.stringify(data) || JSON.stringify(item.metrics)!==JSON.stringify(metrics) || thumbnailName!==item.thumbnail) {
          await request(`/v1/items/${item.id}`,local(signal),{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({item:{...item,data,metrics,thumbnail:thumbnailName,contents:hashes}})});
        }
      }
    },
    async synchronize(quote:PublicationQuote, email:string, provider:Eip1193Provider, signal?:AbortSignal) {
      const p=quote.preparation;
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) throw new Error("Enter an email for Foundation's publication terms.");
      if (p.salt.toLowerCase()!==keccak256(stringToHex(p.id))) throw new Error("The collection salt is incompatible with Foundation Builder. Cancel this attempt and save a new collection draft.");
      const client=createPublicClient({transport:custom(provider,{retryCount:0})});
      const init=encodeFunctionData({abi:initializationAbi,functionName:"initialize",args:[p.name,p.symbol,p.base_uri,p.creator as Address,true,false,quote.rarities as Address,
        p.items.map(item=>({rarity:item.rarity,price:BigInt(item.price),beneficiary:item.beneficiary as Address,metadata:item.metadata}))]});
      const predicted=await client.readContract({address:COLLECTION_FACTORY,abi:factoryAbi,functionName:"getAddress",args:[p.salt as Hex,COLLECTION_FORWARDER,init]});
      const collection=RemoteCollection.parse(await request(`/collections/${p.id}`,remote(signal),{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({
        collection:{id:p.id,name:p.name,eth_address:p.creator,salt:p.salt,contract_address:null,is_published:false,is_approved:false},data:init})}));
      checkCollection(collection,p,predicted);
      for (const expected of p.items) {
        const item=await getItem(expected.id,signal);
        if (!item.metrics || !Object.keys(item.metrics).length) throw new Error(`${item.name}: inspect the model before publishing.`);
        const body={id:item.id,name:item.name,description:item.description,type:item.type,eth_address:p.creator,collection_id:p.id,
          rarity:expected.rarity,price:expected.price,beneficiary:expected.beneficiary,thumbnail:item.thumbnail,data:normalizedData(item),metrics:item.metrics,
          contents:item.contents,is_published:false,is_approved:false};
        await request(`/items/${item.id}`,remote(signal),{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({item:body})});
        await uploadContents(item,signal);
      }
      const rows=z.array(Item).parse(await request(`/collections/${p.id}/items`,remote(signal)));
      rows.sort((a,b)=>new Date(a.created_at??0).getTime()-new Date(b.created_at??0).getTime());
      if (rows.length!==p.items.length || rows.some((item,index)=>item.id!==p.items[index].id)) throw new Error("Foundation item order differs from the reviewed collection. Resolve its drafts before paying.");
      await request(`/collections/${p.id}/tos`,remote(signal),{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({email:email.trim(),event:"publish_collection_tos",collection_address:predicted})});
    },
    async synchronizeLinked(preparation:LinkedPublicationPreparation,email:string,signal?:AbortSignal) {
      const p=LinkedPublicationPreparationSchema.parse(preparation);
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) throw new Error("Enter an email for Foundation's publication terms.");
      z.string().uuid().parse(p.id);
      z.array(z.string().uuid()).min(1).max(50).parse(p.item_ids);
      linkedProviderId.parse(p.third_party_id);
      const suffix=p.urn.slice(p.third_party_id.length+1);
      if (new Set(p.item_ids).size!==p.item_ids.length || !p.urn.startsWith(`${p.third_party_id}:`) || !/^[^:|\s]+$/.test(suffix)) throw new Error("Invalid linked publication preparation.");
      try {
        const existing=LinkedCollection.parse(await request(`/collections/${p.id}`,remote(signal)));
        if (existing.urn!==p.urn || existing.eth_address.toLowerCase()!==p.creator.toLowerCase()) throw new Error("Foundation's linked collection belongs to another draft.");
        if (existing.is_published) throw new Error("This linked collection already has a submission. Check its review status before making changes.");
      } catch (error) {
        if (!(error instanceof CatalystError && error.status===404)) throw error;
      }
      const collection=LinkedCollection.parse(await request(`/collections/${p.id}`,remote(signal),{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({
        collection:{id:p.id,name:p.name,eth_address:p.creator,urn:p.urn,salt:null,contract_address:null,is_published:false,is_approved:false}})}));
      if (collection.id!==p.id || collection.name!==p.name || collection.urn!==p.urn || collection.eth_address.toLowerCase()!==p.creator.toLowerCase()) throw new Error("Foundation's linked collection does not match the reviewed draft.");
      if (collection.is_published) throw new Error("This linked collection already has a submission. Check its review status before making changes.");
      const expected=new Map<string,Record<string,unknown>>();
      for (const id of p.item_ids) {
        const item=await getItem(id,signal);
        if (item.collection_id!==p.id || item.eth_address.toLowerCase()!==p.creator.toLowerCase() || item.is_published) throw new Error("A linked item no longer matches the reviewed draft.");
        if (!item.metrics || !Object.keys(item.metrics).length || !item.thumbnail) throw new Error(`${item.name}: inspect the model before publishing.`);
        const body={id:item.id,urn:`${p.urn}:${item.id}`,name:item.name,description:item.description,type:item.type,eth_address:p.creator,collection_id:p.id,
          rarity:null,price:"0",beneficiary:"0x0000000000000000000000000000000000000000",thumbnail:item.thumbnail,data:normalizedData(item),metrics:item.metrics,
          contents:item.contents,is_published:false,is_approved:false};
        expected.set(id,body);
        await request(`/items/${id}`,remote(signal),{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({item:body})});
        await uploadContents(item,signal);
      }
      const rows=z.array(Item).parse(await request(`/collections/${p.id}/items`,remote(signal)));
      if (rows.length!==expected.size || new Set(rows.map(item=>item.id)).size!==expected.size) throw new Error("Foundation's linked items do not match the reviewed draft.");
      const hashes:string[]=[];
      for (const item of rows) {
        const wanted=expected.get(item.id);
        if (!wanted || item.is_published || ["urn","name","description","type","collection_id","thumbnail","data","metrics","contents"].some(key=>!equalData(item[key as keyof Item],wanted[key]))
          || item.eth_address.toLowerCase()!==p.creator.toLowerCase() || !item.local_content_hash) throw new Error("Foundation's linked item content does not match the reviewed draft.");
        hashes.push(item.local_content_hash);
      }
      await request(`/collections/${p.id}/tos`,remote(signal),{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({
        email:email.trim(),event:"publish_third_party_items_tos",hashes})});
    },
    async submitLinked(p:LinkedPublicationPreparation,cheque:LinkedPublicationCheque,signal?:AbortSignal) {
      if (cheque.qty!==p.item_ids.length) throw new Error("The slot authorization does not match the reviewed items.");
      await request(`/collections/${p.id}/publish`,remote(signal),{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({itemIds:p.item_ids,cheque})});
    },
    async submit(p:PublicationPreparation, contractAddress:string, signal?:AbortSignal) {
      let collection=await remoteCollection(p.id,signal);
      checkCollection(collection,p,contractAddress);
      const published=z.object({collection:RemoteCollection,items:z.array(Item)}).parse(await request(`/collections/${p.id}/publish`,remote(signal),{method:"POST"}));
      checkCollection(published.collection,p,contractAddress);
      if (!published.collection.is_published || published.items.length!==p.items.length || p.items.some((item,index)=>
        published.items.find(remote=>remote.id===item.id)?.blockchain_item_id!==String(index))) throw new Error("Foundation is still indexing the collection. Retry submission; you won't pay again.");
      const forumLink=published.collection.forum_link??collection.forum_link;
      if (forumLink) return {forumUrl:forumURL(forumLink),curationSubmitted:true as const};
      const topic={title:`Collection '${p.name}' created by ${p.creator.slice(0,8)} is ready for review!`,raw:`# ${markdown(p.name)}\n\n[Review collection](https://decentraland.org/builder/item-editor?collection=${encodeURIComponent(p.id)})\n\n`+
        published.items.map(item=>`## ${markdown(item.name)}\n\n${markdown(item.description)}\n\n- Rarity: ${item.rarity}\n- Category: ${markdown(String(item.data.category))}\n\n![${markdown(item.name)}](${FOUNDATION_BUILDER}/storage/contents/${encodeURIComponent(item.contents[item.thumbnail??""]??"")})`).join("\n\n")};
      try {
        return {forumUrl:forumURL(z.string().parse(await request(`/collections/${p.id}/post`,remote(signal),{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({forumPost:topic})}))),curationSubmitted:true as const};
      } catch(error) {
        collection=await remoteCollection(p.id,signal);
        if (collection.forum_link) return {forumUrl:forumURL(collection.forum_link),curationSubmitted:true as const};
        throw error;
      }
    },
  };
}
