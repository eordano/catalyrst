export const CHUNK_SIZE_BYTES = 262144;

export const MAX_CHILDREN_PER_NODE = 174;

const B32_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567";

const getSubtle = (): SubtleCrypto => {
  const c = (globalThis as { crypto?: Crypto }).crypto;
  if (!c?.subtle) {
    throw new Error("hashing: WebCrypto crypto.subtle is unavailable in this runtime");
  }
  return c.subtle;
};

export function base32Lower(bytes: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let out = "";
  for (let i = 0; i < bytes.length; i += 1) {
    value = (value << 8) | bytes[i];
    bits += 8;
    while (bits >= 5) {
      out += B32_ALPHABET[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) {
    out += B32_ALPHABET[(value << (5 - bits)) & 31];
  }
  return out;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const out = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(out).set(bytes);
  return out;
}

const CODEC_RAW = 0x55;
const CODEC_DAG_PB = 0x70;

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await getSubtle().digest("SHA-256", toArrayBuffer(bytes)));
}

function cidBytes(codec: number, digest: Uint8Array): Uint8Array {
  const cid = new Uint8Array(4 + digest.length);
  cid[0] = 0x01;
  cid[1] = codec;
  cid[2] = 0x12;
  cid[3] = 0x20;
  cid.set(digest, 4);
  return cid;
}

export async function hashV1Raw(bytes: Uint8Array): Promise<string> {
  return "b" + base32Lower(cidBytes(CODEC_RAW, await sha256(bytes)));
}


function writeVarint(out: number[], value: number): void {
  let v = value;
  while (v >= 0x80) {
    out.push((v % 0x80) + 0x80);
    v = Math.floor(v / 0x80);
  }
  out.push(v);
}

function unixfsFileData(fileSize: number, blockSizes: readonly number[]): number[] {
  const out: number[] = [0x08, 0x02, 0x18];
  writeVarint(out, fileSize);
  for (const bs of blockSizes) {
    out.push(0x20);
    writeVarint(out, bs);
  }
  return out;
}

interface DagNode {
  cidBytes: Uint8Array;
  tsize: number;
  fileSize: number;
}

function encodePBNode(links: readonly DagNode[], data: readonly number[]): Uint8Array {
  const out: number[] = [];
  for (const l of links) {
    const link: number[] = [0x0a];
    writeVarint(link, l.cidBytes.length);
    for (const b of l.cidBytes) link.push(b);
    link.push(0x12, 0x00, 0x18);
    writeVarint(link, l.tsize);
    out.push(0x12);
    writeVarint(out, link.length);
    for (const b of link) out.push(b);
  }
  if (data.length > 0) {
    out.push(0x0a);
    writeVarint(out, data.length);
    for (const b of data) out.push(b);
  }
  return Uint8Array.from(out);
}

async function reduceBatch(batch: readonly DagNode[]): Promise<DagNode> {
  const blockSizes = batch.map((n) => n.fileSize);
  let fileSize = 0;
  for (const bs of blockSizes) fileSize += bs;
  const block = encodePBNode(batch, unixfsFileData(fileSize, blockSizes));
  let tsize = block.length;
  for (const n of batch) tsize += n.tsize;
  return { cidBytes: cidBytes(CODEC_DAG_PB, await sha256(block)), tsize, fileSize };
}

export async function hashV1(bytes: Uint8Array): Promise<string> {
  if (bytes.length <= CHUNK_SIZE_BYTES) return hashV1Raw(bytes);

  let level: DagNode[] = [];
  for (let off = 0; off < bytes.length; off += CHUNK_SIZE_BYTES) {
    const chunk = bytes.subarray(off, Math.min(off + CHUNK_SIZE_BYTES, bytes.length));
    level.push({
      cidBytes: cidBytes(CODEC_RAW, await sha256(chunk)),
      tsize: chunk.length,
      fileSize: chunk.length,
    });
  }

  for (;;) {
    const next: DagNode[] = [];
    for (let i = 0; i < level.length; i += MAX_CHILDREN_PER_NODE) {
      next.push(await reduceBatch(level.slice(i, i + MAX_CHILDREN_PER_NODE)));
    }
    level = next;
    if (level.length === 1) break;
  }
  return "b" + base32Lower(level[0].cidBytes);
}

export async function hashFile(bytes: Uint8Array): Promise<string> {
  return hashV1(bytes);
}

export function needsMultiBlockHash(bytes: Uint8Array): boolean {
  return bytes.length > CHUNK_SIZE_BYTES;
}

export class MultiBlockHashUnsupportedError extends Error {
  readonly byteLength: number;
  constructor(byteLength: number) {
    super(
      `hashFile: ${byteLength} bytes exceeds the ${CHUNK_SIZE_BYTES}-byte single-block limit.`,
    );
    this.name = "MultiBlockHashUnsupportedError";
    this.byteLength = byteLength;
  }
}

export function utf8(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}
