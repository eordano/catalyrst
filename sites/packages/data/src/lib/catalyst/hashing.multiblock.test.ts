import { describe, expect, it } from "vitest";

import { hashFile, hashV1, MAX_CHILDREN_PER_NODE } from "./hashing";


function gen(size: number): Uint8Array {
  const b = new Uint8Array(size);
  for (let i = 0; i < size; i += 1) b[i] = ((i * 2654435761) >>> 0) & 255;
  return b;
}

const ORACLE: Array<[number, string]> = [
  [1, "bafkreidogqfzz75tpkmjzjke425xqcrmpcib2p5tg44hnbirumdbpl5adu"],
  [262144, "bafkreihg5ejywhb2ion5vir77fkn4qzjk3ifgosiowlvlrcw7r24gi5a7e"],
  [262145, "bafybeiajcvhuc3qypyefmhw43blh7eswfkg2oif3wb6pcwz2mnvv3zcpbq"],
  [300000, "bafybeihefjinfe5hjuc4oq5s3nfjhvqirc74fpnbsucz7it26whygcqapq"],
  [1_000_000, "bafybeibzkbowoigzk4wlvh2sybr3yn2ywbeuuxivvac6tt2matk4l5jvpi"],
  [5_000_000, "bafybeihrlekxphusf2zc74jg27r3wvqrxxiu3waweymj6shfhnveadm7vi"],
  [13_000_000, "bafybeifihom44evfpr6ek3vqwkijdhtpbdttlujr233mygxwae57tz5sie"],
  [45_000_000, "bafybeiaibbr26nh7cdry3z567kaxwo7g2exavl7ekqe3ond4cpkjdurvf4"],
  [45_613_057, "bafybeiailuqbfoq273oqzohlzhxt7c5gd3ax3zkwbfbawye2kanfwdskey"],
  [50_000_000, "bafybeicneokpax72wvvqicxvd5cmtpzdnpmpouhegbfxcelp4jr726aqhi"],
  [100_000_000, "bafybeif422bs55lv3xosnrjcda6nqpsd2jq5y4cfq5ogs2wadr3vhkpol4"],
];

describe("hashV1 (multi-block) is byte-exact with @dcl/hashing", () => {
  for (const [size, expected] of ORACLE) {
    it(`size ${size} \u{2192} ${expected}`, async () => {
      const bytes = gen(size);
      expect(await hashV1(bytes)).toBe(expected);
      expect(await hashFile(bytes)).toBe(expected);
    }, 30_000);
  }

  it("uses the importer's default fan-out (maxChildrenPerNode = 174)", () => {
    expect(MAX_CHILDREN_PER_NODE).toBe(174);
  });
});
