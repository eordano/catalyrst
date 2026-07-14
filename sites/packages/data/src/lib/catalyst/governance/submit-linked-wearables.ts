import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-linked-wearables.json";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import { ETH_ADDRESS_RE } from "../format/address";

const TextFieldSchema = z.object({
  label: z.string(),
  detail: z.string(),
  placeholder: z.string().nullish(),
  min_length: z.number().nullish(),
  max_length: z.number().nullish(),
  required: z.boolean(),
  markdown: z.boolean(),
});

const ListFieldSchema = z.object({
  label: z.string(),
  detail: z.string(),
  placeholder: z.string().nullish(),
  add_label: z.string().nullish(),
  format: z.string().nullish(),
  min_items: z.number().nullish(),
  max_items: z.number().nullish(),
  required: z.boolean(),
  markdown: z.boolean(),
});

const RadioFieldSchema = z.object({
  label: z.string(),
  detail: z.string(),
  note: z.string(),
  yes_label: z.string(),
  no_label: z.string(),
  required: z.boolean(),
  markdown: z.boolean(),
});

const SampleSchema = z.object({
  name: z.string(),
  marketplace_link: z.string(),
  links: z.array(z.string()),
  image_previews: z.array(z.string()),
  nft_collections: z.string(),
  motivation: z.string(),
  items: z.string(),
  governance: z.string(),
  smart_contract: z.array(z.string()),
  managers: z.array(z.string()),
  programmatically_generated: z.boolean(),
  method: z.string(),
  coAuthors: z.array(z.string()),
});

const LinkedWearablesSchema = z.object({
  type: z.string(),
  title: z.string(),
  description: z.string(),
  description_note: z.string(),
  submit_label: z.string(),
  fields: z.object({
    name: TextFieldSchema,
    marketplace_link: ListFieldSchema.partial({ add_label: true }).and(
      z.object({ format: z.string().nullish() }),
    ),
    links: ListFieldSchema,
    image_previews: ListFieldSchema,
    nft_collections: TextFieldSchema,
    motivation: TextFieldSchema,
    items: z.object({
      label: z.string(),
      detail: z.string(),
      placeholder: z.string().nullish(),
      minimum: z.number(),
      maximum: z.number(),
      required: z.boolean(),
      markdown: z.boolean(),
    }),
    governance: TextFieldSchema,
    smart_contract: ListFieldSchema,
    managers: ListFieldSchema,
    programmatically_generated: RadioFieldSchema,
    method: TextFieldSchema.and(z.object({ conditional_on: z.string().nullish() })),
    coAuthors: z.object({
      label: z.string(),
      detail: z.string(),
      placeholder: z.string().nullish(),
      max: z.number(),
      required: z.boolean(),
      optional: z.boolean(),
      markdown: z.boolean(),
    }),
  }),
  errors: z.record(z.string(), z.string()),
  submit_error: z.string(),
  sample: SampleSchema,
  success: z.object({
    title: z.string(),
    lead: z.string(),
    note: z.string(),
  }),
});

export type LinkedWearablesData = z.infer<typeof LinkedWearablesSchema>;
export type LinkedWearablesSample = z.infer<typeof SampleSchema>;

const FALLBACK: LinkedWearablesData = {
  type: "linked_wearables",
  title: "Linked Wearables Registry",
  description:
    "Linked Wearables are a way to represent NFTs as Wearables in Decentraland. Third parties need to submit a proposal to be approved by the DAO in order to access the tool in the Builder and get slots to submit the 3D models.",
  description_note:
    "Note that after being approved, you will need to create an API with the endpoints described in the ADR-42 third-party assets integration document.",
  submit_label: "Submit proposal",
  fields: {
    name: {
      label: "Name",
      detail:
        "Please enter the name that represents your Project, Company, or Community as a whole",
      placeholder: "Enter the name here",
      min_length: 1,
      max_length: 80,
      required: true,
      markdown: false,
    },
    marketplace_link: {
      label: "NFT Marketplace Listing",
      detail:
        "Provide an URL where users can see your NFT collection listed in an NFT marketplace like OpenSea, Rarible, or any other",
      placeholder: "Add a link here",
      format: "url",
      required: true,
      markdown: false,
    },
    links: {
      label: "Links",
      detail:
        "Links for your project website, Discord server, social media, or any other relevant space for your Project",
      placeholder: "Add a link here",
      add_label: "Add another link",
      format: "url",
      min_items: 1,
      required: true,
      markdown: false,
    },
    image_previews: {
      label: "Collection Images",
      detail:
        "Provide up to {amount} images to show the community what your collection and wearables looks like. JPG, PNG, BMP & WEBP formats supported.",
      placeholder: "Insert image URL",
      add_label: "Add another image",
      format: "image",
      min_items: 1,
      max_items: 10,
      required: true,
      markdown: false,
    },
    nft_collections: {
      label: "NFT Collections",
      detail:
        "Describe the NFT Collections you created. If it\u{2019}s only one, just describe that one",
      placeholder: "Describe the NFT Collections you created here",
      min_length: 20,
      max_length: 750,
      required: true,
      markdown: true,
    },
    motivation: {
      label: "Motivation",
      detail: "Why do you want to have your NFTs represented in Decentraland?",
      placeholder: "A brief motivation",
      min_length: 20,
      max_length: 750,
      required: true,
      markdown: true,
    },
    items: {
      label: "Items in Linked Wearables Collection",
      detail:
        "How many 3D models (Linked Wearables) will be uploaded. Note: This is not the number of NFTs in the original collection.",
      placeholder: "Add the number of items you will upload",
      minimum: 1,
      maximum: 99999,
      required: true,
      markdown: false,
    },
    governance: {
      label: "Intellectual Property",
      detail:
        "Provide proof that you are the rightful owner or representative of the Project, Company, or Community. Please share any links with relevant information.",
      placeholder: "Provide proof here",
      min_length: 20,
      max_length: 750,
      required: true,
      markdown: true,
    },
    smart_contract: {
      label: "Smart Contracts",
      detail: "Share the Addresses of the smart contracts of your NFT collections",
      placeholder: "Add Ethereum address",
      add_label: "Add another address",
      format: "address",
      min_items: 1,
      required: true,
      markdown: false,
    },
    managers: {
      label: "Managers",
      detail:
        "Addresses of the representatives that will Manage the tool. Note: Managers are the only ones allowed to add item representations and manage the tool",
      placeholder: "Add Ethereum address",
      add_label: "Add another address",
      format: "address",
      min_items: 1,
      required: true,
      markdown: false,
    },
    programmatically_generated: {
      label: "Is this collection programmatically generated?",
      detail:
        "The collection you will upload to Decentraland as Linked Wearables is programmatically generated. This means the 3D models you will submit to Decentraland were made this way.",
      note: "In general, large collections of more than 5k NFTs are not created manually, they are systematically generated from individual traits that were designed individually. If your Linked Wearables Collection is made like this, share the details below.",
      yes_label: "Yes",
      no_label: "No",
      required: true,
      markdown: false,
    },
    method: {
      label: "Method",
      detail:
        "Describe the method used to create the programmatic collection. If possible, share proof and links to the repository.",
      placeholder: "Describe the method",
      min_length: 0,
      max_length: 750,
      required: false,
      conditional_on: "programmatically_generated",
      markdown: true,
    },
    coAuthors: {
      label: "Co-Authors",
      detail: "Add the Decentraland addresses of any co-authors of this proposal",
      placeholder: "Add Ethereum address",
      max: 5,
      required: false,
      optional: true,
      markdown: false,
    },
  },
  errors: {},
  submit_error:
    "There was an error while trying to create the proposal, please try again later.",
  sample: {
    name: "",
    marketplace_link: "",
    links: [],
    image_previews: [],
    nft_collections: "",
    motivation: "",
    items: "1",
    governance: "",
    smart_contract: [],
    managers: [],
    programmatically_generated: false,
    method: "",
    coAuthors: [],
  },
  success: {
    title: "Proposal created",
    lead: "Your Linked Wearables Registry proposal is now live for the DAO to vote on.",
    note: "This proposal was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
  },
};

function parse(): LinkedWearablesData {
  const parsed = LinkedWearablesSchema.safeParse(fixture);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getLinkedWearablesData(): LinkedWearablesData {
  return parse();
}

export const MAX_IMAGES = 10;

export function isHttpsURL(url: string): boolean {
  const u = url.trim();
  if (!/^https:\/\//i.test(u)) return false;
  try {
    const parsed = new URL(u);
    return parsed.protocol === "https:" && parsed.hostname.includes(".");
  } catch {
    return false;
  }
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

export function asNumber(value: string): number {
  return Number(value);
}

export function validateImageUrl(url: string): boolean {
  const u = url.trim();
  if (!u) return false;
  if (!isHttpsURL(u)) return false;
  return /\.(jpe?g|png|bmp|webp)(\?.*)?$/i.test(u);
}

export type IdentityInput = {
  name: string;
  marketplaceLink: string;
  links: string[];
};

export type FieldErrors = Record<string, string>;

const ERR = (data: LinkedWearablesData, key: string, fallback: string) =>
  data.errors[key] ?? fallback;

export function validateIdentity(input: IdentityInput): FieldErrors {
  const data = parse();
  const errors: FieldErrors = {};
  const name = input.name.trim();
  const min = data.fields.name.min_length ?? 1;
  const max = data.fields.name.max_length ?? 80;
  if (!name) errors.name = ERR(data, "name_empty", "Name is empty");
  else if (name.length < min) errors.name = ERR(data, "name_too_short", "Name is too short");
  else if (name.length > max) errors.name = ERR(data, "name_too_large", "Name is too large");

  const link = input.marketplaceLink.trim();
  if (!link) errors.marketplace_link = ERR(data, "single_url_empty", "Insert an URL");
  else if (!isHttpsURL(link))
    errors.marketplace_link = ERR(
      data,
      "single_url_invalid",
      "The URL is invalid, it must start with 'https://'",
    );

  const links = input.links.map((l) => l.trim()).filter(Boolean);
  if (links.length < 1) errors.links = ERR(data, "links_empty", "Insert at least one URL");
  else if (links.some((l) => !isHttpsURL(l)))
    errors.links = ERR(
      data,
      "url_invalid",
      "Some URL is invalid, they must start with 'https://'",
    );

  return errors;
}

export type CollectionInput = {
  imagePreviews: string[];
  nftCollections: string;
  items: string;
};

export function validateCollection(input: CollectionInput): FieldErrors {
  const data = parse();
  const errors: FieldErrors = {};

  const images = input.imagePreviews.map((i) => i.trim()).filter(Boolean);
  if (images.length < 1) errors.image_previews = ERR(data, "links_empty", "Insert at least one URL");
  else if (images.length > MAX_IMAGES)
    errors.image_previews = `You can add up to ${MAX_IMAGES} images.`;
  else if (images.some((i) => !validateImageUrl(i)))
    errors.image_previews = ERR(
      data,
      "image_type_invalid",
      "Some image is invalid, it must be a JPG, PNG, BMP or WEBP",
    );

  const nft = input.nftCollections.trim();
  const min = data.fields.nft_collections.min_length ?? 20;
  const maxLen = data.fields.nft_collections.max_length ?? 750;
  if (!nft) errors.nft_collections = ERR(data, "nft_collections_empty", "Description is empty");
  else if (nft.length < min)
    errors.nft_collections = ERR(data, "nft_collections_too_short", "Description is too short");
  else if (nft.length > maxLen)
    errors.nft_collections = ERR(data, "nft_collections_too_large", "Description is too large");

  const n = asNumber(input.items);
  const minItems = data.fields.items.minimum;
  const maxItems = data.fields.items.maximum;
  if (!Number.isFinite(n) || !Number.isInteger(n))
    errors.items = ERR(data, "items_invalid", "Items quantity is not a valid number");
  else if (n < minItems) errors.items = ERR(data, "items_too_low", "Items quantity is too low");
  else if (n > maxItems) errors.items = ERR(data, "items_too_high", "Items quantity is too high");

  return errors;
}

export type TechnicalInput = {
  smartContracts: string[];
  managers: string[];
  programmaticallyGenerated: boolean;
  method: string;
};

export function validateTechnical(input: TechnicalInput): FieldErrors {
  const data = parse();
  const errors: FieldErrors = {};

  const contracts = input.smartContracts.map((c) => c.trim()).filter(Boolean);
  if (contracts.length < 1)
    errors.smart_contract = ERR(
      data,
      "smart_contract_empty",
      "Insert at least one smart contract address",
    );
  else if (contracts.some((c) => !isEthAddress(c)))
    errors.smart_contract = ERR(data, "address_invalid", "Some address is invalid");

  const managers = input.managers.map((m) => m.trim()).filter(Boolean);
  if (managers.length < 1)
    errors.managers = ERR(data, "managers_empty", "Insert at least one manager address");
  else if (managers.some((m) => !isEthAddress(m)))
    errors.managers = ERR(data, "address_invalid", "Some address is invalid");

  if (input.programmaticallyGenerated) {
    const method = input.method.trim();
    const min = data.fields.method.min_length ?? 0;
    const max = data.fields.method.max_length ?? 750;
    if (!method) errors.method = ERR(data, "method_empty", "Method is empty");
    else if (method.length < min) errors.method = ERR(data, "method_too_short", "Method is too short");
    else if (method.length > max) errors.method = ERR(data, "method_too_large", "Method is too large");
  }

  return errors;
}

export type NewProposalLinkedWearables = {
  type: string;
  name: string;
  marketplace_link: string;
  links: string[];
  image_previews: string[];
  nft_collections: string;
  motivation: string;
  items: number;
  governance: string;
  smart_contract: string[];
  managers: string[];
  programmatically_generated: boolean;
  method: string;
  coAuthors: string[];
};

export type CreatedProposal = {
  id: string;
  type: string;
};

export type LinkedWearablesDraft = {
  identity: IdentityInput;
  collection: CollectionInput;
  technical: TechnicalInput;
  coAuthors: string[];
};

const removeEmpty = (arr: string[]) => arr.map((s) => s.trim()).filter(Boolean);

export function buildProposalPayload(
  draft: LinkedWearablesDraft,
): NewProposalLinkedWearables {
  return {
    type: "linked_wearables",
    name: draft.identity.name.trim(),
    marketplace_link: draft.identity.marketplaceLink.trim(),
    links: removeEmpty(draft.identity.links),
    image_previews: removeEmpty(draft.collection.imagePreviews),
    nft_collections: draft.collection.nftCollections.trim(),
    motivation: "",
    items: asNumber(draft.collection.items),
    governance: "",
    smart_contract: removeEmpty(draft.technical.smartContracts),
    managers: removeEmpty(draft.technical.managers),
    programmatically_generated: draft.technical.programmaticallyGenerated,
    method: draft.technical.programmaticallyGenerated ? draft.technical.method.trim() : "",
    coAuthors: removeEmpty(draft.coAuthors),
  };
}

export type CreateProposalFn = (args: {
  payload: NewProposalLinkedWearables;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

const SUBMIT_UNAVAILABLE =
  "linked wearables proposal submission unavailable: DAO governance signer not configured";

export const failClosedCreateProposal: CreateProposalFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateProposal(
  identity: AuthIdentity | null,
): CreateProposalFn {
  return async ({ payload, signal }) => {
    const created = await submitProposal({
      identity,
      kind: "linked-wearables",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return { id: created.id, type: created.type ?? payload.type };
  };
}
