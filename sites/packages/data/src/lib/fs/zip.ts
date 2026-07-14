const enc = new TextEncoder();

let CRC_TABLE: Uint32Array | null = null;
function crcTable(): Uint32Array {
  if (CRC_TABLE) return CRC_TABLE;
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  CRC_TABLE = t;
  return t;
}

export function crc32(bytes: Uint8Array): number {
  const t = crcTable();
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = t[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export type ZipEntry = { path: string; text: string };

const u16 = (n: number) => new Uint8Array([n & 0xff, (n >>> 8) & 0xff]);
const u32 = (n: number) =>
  new Uint8Array([n & 0xff, (n >>> 8) & 0xff, (n >>> 16) & 0xff, (n >>> 24) & 0xff]);

function concat(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

export function makeZip(entries: ZipEntry[]): Uint8Array {
  const local: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  const DATE = 0x0021;

  for (const e of entries) {
    const name = enc.encode(e.path);
    const data = enc.encode(e.text);
    const crc = crc32(data);
    const size = data.length;

    const lfh = concat([
      u32(0x04034b50), u16(20), u16(0), u16(0),
      u16(0), u16(DATE),
      u32(crc), u32(size), u32(size),
      u16(name.length), u16(0),
      name,
    ]);
    local.push(lfh, data);

    central.push(
      concat([
        u32(0x02014b50), u16(20), u16(20), u16(0), u16(0),
        u16(0), u16(DATE),
        u32(crc), u32(size), u32(size),
        u16(name.length), u16(0), u16(0),
        u16(0), u16(0), u32(0),
        u32(offset),
        name,
      ]),
    );
    offset += lfh.length + data.length;
  }

  const centralBytes = concat(central);
  const eocd = concat([
    u32(0x06054b50), u16(0), u16(0),
    u16(entries.length), u16(entries.length),
    u32(centralBytes.length), u32(offset),
    u16(0),
  ]);

  return concat([...local, centralBytes, eocd]);
}
