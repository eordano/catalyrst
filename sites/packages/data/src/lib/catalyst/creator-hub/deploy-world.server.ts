import {
  fetchOwnedNames,
  fetchWorldsRealm,
  MAX_FILE_SIZE_MB,
  normalizeAddress,
  shortAddress,
  withPersonalWorld,
  WORLDS_REALM_UNKNOWN,
  type DeployName,
  type DeployWorldData,
  type WorldsRealm,
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

export function fallbackDeployWorld(address: string | null | undefined): DeployWorldData {
  const addr = normalizeAddress(address);
  return {
    address: addr,
    names: [],
    liveEmpty: true,
    worldsOnline: null,
    personalWorlds: false,
    project: UNKNOWN_PROJECT,
    files: [],
    maxFileSizeMb: MAX_FILE_SIZE_MB,
    owner: ownerFor(addr),
    source: "empty",
  };
}

export async function loadDeployWorld(
  address: string | null | undefined,
  opts: GetOptions = {},
): Promise<DeployWorldData> {
  const addr = normalizeAddress(address);

  let ownedNames: DeployName[] = [];
  let source: "live" | "empty" = "empty";
  if (addr) {
    try {
      const page = await fetchOwnedNames(addr, opts);
      source = "live";
      ownedNames = page.elements.map((n) => ({
        name: `${n.name.trim().toLowerCase().replace(/\.(dcl\.eth|eth)$/i, "")}.dcl.eth`,
        provider: "dcl" as const,
        world: null,
      }));
    } catch {
      ownedNames = [];
      source = "empty";
    }
  }
  let realm: WorldsRealm = WORLDS_REALM_UNKNOWN;
  try {
    realm = await fetchWorldsRealm(opts);
  } catch {
    realm = WORLDS_REALM_UNKNOWN;
  }
  const liveNames = withPersonalWorld(addr, ownedNames, realm.personalWorlds);

  return {
    address: addr,
    names: liveNames,
    liveEmpty: liveNames.length === 0,
    worldsOnline: realm.online,
    personalWorlds: realm.personalWorlds,
    project: UNKNOWN_PROJECT,
    files: [],
    maxFileSizeMb: MAX_FILE_SIZE_MB,
    owner: ownerFor(addr),
    source,
  };
}
