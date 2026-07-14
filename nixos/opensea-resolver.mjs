import { createServer } from 'node:http';
import { execFile } from 'node:child_process';
import { isIP } from 'node:net';

const PORT = Number(process.env.PORT ?? 5162);
const DOMAIN = process.env.DOMAIN ?? 'decentraland.org';
const PSQL = process.env.PSQL ?? 'psql';
const PG_CONN = process.env.PG_CONN ?? '';
const RPC = {
  ethereum: process.env.RPC_MAINNET ?? 'https://rpc.decentraland.org/mainnet',
  matic: process.env.RPC_POLYGON ?? 'https://rpc.decentraland.org/polygon',
  polygon: process.env.RPC_POLYGON ?? 'https://rpc.decentraland.org/polygon',
};
const NETWORK = { ethereum: 'ETHEREUM', matic: 'POLYGON', polygon: 'POLYGON' };
const CRYPTOKITTIES = '0x06012c8cf97bead5deae237070f9587f8e7a266d';
const IPFS_GATEWAYS = [
  'https://ipfs.io/ipfs/',
  'https://dweb.link/ipfs/',
  'https://w3s.link/ipfs/',
  'https://nftstorage.link/ipfs/',
];

function psqlJson(sql) {
  return new Promise((resolve) => {
    execFile(PSQL, [PG_CONN, '-Atc', sql], { timeout: 5000 }, (err, stdout) => {
      if (err) return resolve(null);
      const line = stdout.trim();
      if (!line) return resolve(null);
      try {
        resolve(JSON.parse(line));
      } catch {
        resolve(null);
      }
    });
  });
}

// The systemd sandbox must ALLOW localhost (else nginx cannot reach this
// listener), which re-permits loopback egress, so the SSRF guard against an
// attacker-influenced tokenURI lives here: refuse any hop whose host is a
// loopback/private/link-local literal or resolves to `localhost`. link-local
// and RFC1918 are still denied at the network layer too (defense in depth).
function hostIsInternal(rawHost) {
  const h = rawHost.replace(/^\[|\]$/g, '').toLowerCase();
  if (h === 'localhost' || h.endsWith('.localhost') || h === '') return true;
  if (isIP(h) === 0) return false; // a name we won't pre-resolve; net layer guards it
  return (
    /^127\./.test(h) ||
    h === '::1' ||
    h === '0.0.0.0' ||
    /^10\./.test(h) ||
    /^172\.(1[6-9]|2\d|3[01])\./.test(h) ||
    /^192\.168\./.test(h) ||
    /^169\.254\./.test(h) ||
    /^fe80:/.test(h) ||
    /^f[cd][0-9a-f][0-9a-f]:/.test(h)
  );
}

function urlBlocked(u) {
  try {
    return hostIsInternal(new URL(u).hostname);
  } catch {
    return true;
  }
}

async function fetchText(url, ms, accept) {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), ms);
  try {
    let current = url;
    for (let hop = 0; hop < 5; hop++) {
      if (!current.startsWith('data:') && urlBlocked(current)) return null;
      const res = await fetch(current, {
        signal: ctl.signal,
        redirect: 'manual',
        headers: accept ? { accept } : {},
      });
      if (res.status >= 300 && res.status < 400) {
        const loc = res.headers.get('location');
        if (!loc) return null;
        current = new URL(loc, current).toString();
        continue;
      }
      if (!res.ok) return null;
      if (Number(res.headers.get('content-length') ?? 0) > 2_000_000) return null;
      return await res.text();
    }
    return null;
  } catch {
    return null;
  } finally {
    clearTimeout(t);
  }
}

function ipfsPath(uri) {
  const m = uri.match(/^ipfs:\/\/(?:ipfs\/)?(.+)$/);
  return m ? m[1] : null;
}

function toHttp(uri, gateway = IPFS_GATEWAYS[0]) {
  if (!uri || typeof uri !== 'string') return null;
  const p = ipfsPath(uri);
  if (p) return gateway + p;
  if (uri.startsWith('ar://')) return 'https://arweave.net/' + uri.slice(5);
  if (/^https?:\/\//.test(uri) || uri.startsWith('data:')) return uri;
  return null;
}

async function fetchMetadata(uri) {
  const inline = uri.match(/^data:application\/json(?:;charset=[^;,]+)?(;base64)?,(.*)$/s);
  if (inline) {
    try {
      const body = inline[1] ? Buffer.from(inline[2], 'base64').toString() : decodeURIComponent(inline[2]);
      return { meta: JSON.parse(body), gateway: null };
    } catch {
      return null;
    }
  }
  const p = ipfsPath(uri);
  const attempts = p ? IPFS_GATEWAYS.map((g) => ({ url: g + p, gateway: g })) : [{ url: toHttp(uri), gateway: null }];
  for (const { url, gateway } of attempts) {
    if (!url) continue;
    const text = await fetchText(url, p ? 6000 : 10000, 'application/json');
    if (text) {
      try {
        return { meta: JSON.parse(text), gateway };
      } catch {}
    }
  }
  return null;
}

async function ethCall(rpc, to, data) {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), 6000);
  try {
    const res = await fetch(rpc, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_call', params: [{ to, data }, 'latest'] }),
      signal: ctl.signal,
    });
    const json = await res.json();
    return typeof json.result === 'string' && json.result.length > 2 ? json.result : null;
  } catch {
    return null;
  } finally {
    clearTimeout(t);
  }
}

function decodeAbiString(hex) {
  if (!hex) return null;
  try {
    const b = Buffer.from(hex.slice(2), 'hex');
    if (b.length < 64) return null;
    const off = Number(BigInt('0x' + b.subarray(0, 32).toString('hex')));
    const len = Number(BigInt('0x' + b.subarray(off, off + 32).toString('hex')));
    const s = b.subarray(off + 32, off + 32 + len).toString('utf8');
    return s.length ? s : null;
  } catch {
    return null;
  }
}

function nftResponse({ chain, contract, id, name, description, image, animation, metadataUrl, standard }) {
  return {
    nft: {
      identifier: id,
      collection: contract,
      contract,
      token_standard: standard,
      name: name ?? null,
      description: description ?? null,
      image_url: image ?? null,
      display_image_url: image ?? null,
      animation_url: animation ?? null,
      display_animation_url: animation ?? null,
      metadata_url: metadataUrl ?? null,
      opensea_url: `https://opensea.io/assets/${chain}/${contract}/${id}`,
      updated_at: '',
      is_disabled: false,
      is_nsfw: false,
      is_suspicious: false,
      creator: null,
      traits: [],
      owners: [],
      rarity: null,
    },
  };
}

const rewriteOurs = (u) => (u ? u.replaceAll('https://peer.decentraland.org/', `https://${DOMAIN}/`) : u);

async function resolveNft(chain, contract, id) {
  const row = await psqlJson(
    `SELECT row_to_json(t) FROM (SELECT name, image, token_uri FROM squid_marketplace.nft ` +
      `WHERE contract_address = '${contract}' AND token_id = '${id}' AND network = '${NETWORK[chain]}' LIMIT 1) t`,
  );
  if (row) {
    return nftResponse({
      chain,
      contract,
      id,
      name: row.name,
      image: rewriteOurs(row.image),
      metadataUrl: rewriteOurs(row.token_uri),
      standard: 'erc721',
    });
  }
  if (chain === 'ethereum' && contract === CRYPTOKITTIES) {
    const text = await fetchText(`https://api.cryptokitties.co/v3/kitties/${id}`, 10000, 'application/json');
    if (text) {
      try {
        const k = JSON.parse(text);
        return nftResponse({
          chain,
          contract,
          id,
          name: k.name,
          description: k.bio,
          image: k.image_url_png ?? k.image_url,
          standard: 'erc721',
        });
      } catch {}
    }
  }
  const idHex = BigInt(id).toString(16).padStart(64, '0');
  let standard = 'erc721';
  let uri = decodeAbiString(await ethCall(RPC[chain], contract, '0xc87b56dd' + idHex));
  if (!uri) {
    uri = decodeAbiString(await ethCall(RPC[chain], contract, '0x0e89341c' + idHex));
    if (uri) {
      standard = 'erc1155';
      uri = uri.replaceAll('{id}', idHex);
    }
  }
  if (!uri) return null;
  const fetched = await fetchMetadata(uri);
  if (!fetched) return null;
  const { meta, gateway } = fetched;
  const gw = gateway ?? IPFS_GATEWAYS[0];
  return nftResponse({
    chain,
    contract,
    id,
    name: meta.name,
    description: meta.description,
    image: toHttp(meta.image ?? meta.image_url, gw),
    animation: toHttp(meta.animation_url, gw),
    metadataUrl: toHttp(uri, gw),
    standard,
  });
}

const ROUTE = /^\/api\/v2\/chain\/(ethereum|matic|polygon)\/contract\/(0x[0-9a-fA-F]{40})\/nfts\/(\d+)$/;

createServer(async (req, res) => {
  const send = (code, obj) => {
    res.writeHead(code, { 'content-type': 'application/json' });
    res.end(JSON.stringify(obj));
  };
  const m = new URL(req.url, 'http://localhost').pathname.match(ROUTE);
  if (req.url === '/health') return send(200, { ok: true });
  if (!m) return send(404, { errors: ['unknown route'] });
  try {
    const out = await resolveNft(m[1], m[2].toLowerCase(), m[3]);
    if (out) return send(200, out);
    return send(404, { errors: ['NFT not found'] });
  } catch (e) {
    return send(500, { errors: [String(e?.message ?? e)] });
  }
}).listen(PORT, '127.0.0.1');
