import {
  fetchOwnedNames,
  fetchWorldsOnline,
  MAX_FILE_SIZE_MB,
  normalizeAddress,
  shortAddress,
  type DeployName,
  type DeployWorldData,
} from "./deploy-world";
import type { GetOptions } from "../client";


const UNKNOWN_PROJECT: DeployWorldData["project"] = {
  title: "Your scene",
  size: "",
  grad: "linear-gradient(135deg, #ff2d55 0%, #350447 100%)",
};

function ownerFor(addr: string): DeployWorldData["owner"] {
  return {
    network: "Mainnet",
    address: addr ? shortAddress(addr) : "0x\u{2026}",
    username: "",
    verified: false,
    role: "Owner",
  };
}

export async function loadDeployWorld(
  address: string | null | undefined,
  opts: GetOptions = {},
): Promise<DeployWorldData> {
  const addr = normalizeAddress(address);

  let liveNames: DeployName[] = [];
  let liveEmpty = true;
  let source: "live" | "empty" = "empty";
  if (addr) {
    try {
      const page = await fetchOwnedNames(addr, opts);
      source = "live";
      liveEmpty = page.elements.length === 0;
      liveNames = page.elements.map((n) => ({
        name: `${n.name.trim().toLowerCase().replace(/\.(dcl\.eth|eth)$/i, "")}.dcl.eth`,
        provider: "dcl" as const,
        world: null,
      }));
    } catch {
      liveNames = [];
      liveEmpty = true;
      source = "empty";
    }
  }

  let worldsOnline: boolean | null = null;
  try {
    worldsOnline = await fetchWorldsOnline(opts);
  } catch {
    worldsOnline = false;
  }

  return {
    address: addr,
    names: liveNames,
    liveEmpty,
    worldsOnline,
    project: UNKNOWN_PROJECT,
    files: [],
    maxFileSizeMb: MAX_FILE_SIZE_MB,
    owner: ownerFor(addr),
    source,
  };
}
