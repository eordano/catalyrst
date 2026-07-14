import { Coordinate } from "../../types";

const clearLow = BigInt(
  "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF00000000000000000000000000000000"
);
const clearHigh = BigInt(
  "0x00000000000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"
);

function unsafeDecodeTokenId(value: bigint): [number, number] {
  const x = expandNegative128BitCast((value & clearLow) >> BigInt(128));
  const y = expandNegative128BitCast(value & clearHigh);
  return [x, y];
}

function expandNegative128BitCast(value: bigint): number {
  const bit128 = BigInt(1) << BigInt(127);
  const mask128 = bit128 - BigInt(1);
  if ((value & bit128) !== BigInt(0)) {
    return Number(value | ~mask128);
  }
  return Number(value);
}

function decodeTokenId(value: bigint): [number, number] {
  const [x, y] = unsafeDecodeTokenId(value);
  if (-1000000 < x && x < 1000000 && -1000000 < y && y < 1000000) {
    return [x, y];
  } else {
    throw new Error("The coordinates should be inside bounds");
  }
}

export function decodeTokenIdsToCoordinates(
  tokenIds: Set<bigint>
): Map<bigint, Coordinate> {
  const map = new Map();
  for (const tokenId of tokenIds) {
    const coords = decodeTokenId(tokenId);
    map.set(tokenId, { _0: coords[0], _1: coords[1] });
  }
  return map;
}
