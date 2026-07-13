use abgen::unity::bundle_file::{Bundle, FileContent};
use anyhow::Result;

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn cstr(&mut self) -> String {
        let s = self.p;
        while self.d[self.p] != 0 {
            self.p += 1;
        }
        let out = String::from_utf8_lossy(&self.d[s..self.p]).into_owned();
        self.p += 1;
        out
    }
    fn u16be(&mut self) -> u16 {
        let v = u16::from_be_bytes(self.d[self.p..self.p + 2].try_into().unwrap());
        self.p += 2;
        v
    }
    fn u32be(&mut self) -> u32 {
        let v = u32::from_be_bytes(self.d[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    fn i64be(&mut self) -> i64 {
        let v = i64::from_be_bytes(self.d[self.p..self.p + 8].try_into().unwrap());
        self.p += 8;
        v
    }
    fn skip(&mut self, n: usize) {
        self.p += n;
    }
    fn align(&mut self, a: usize) {
        self.p = self.p.div_ceil(a) * a;
    }
}

fn container(data: &[u8]) -> Result<()> {
    let mut r = Rd { d: data, p: 0 };
    let sig = r.cstr();
    let fv = r.u32be();
    let vp = r.cstr();
    let ve = r.cstr();
    let total = r.i64be();
    let cbi = r.u32be();
    let ubi = r.u32be();
    let flags = r.u32be();
    if fv >= 7 {
        r.align(16);
    }
    println!(
        "  container: sig={sig} fmt={fv} player={vp} engine={ve} total_size={total} blocksinfo comp={}B uncomp={}B flags=0x{flags:x} (blocksinfo_comp_type={})",
        cbi,
        ubi,
        flags & 0x3f
    );
    let bi_raw: &[u8];
    if flags & 0x80 != 0 {
        bi_raw = &data[data.len() - cbi as usize..];
    } else {
        bi_raw = &data[r.p..r.p + cbi as usize];
        r.skip(cbi as usize);
    }
    let bi = match flags & 0x3f {
        0 => bi_raw.to_vec(),
        2 | 3 => {
            abgen::lz4::decompress(bi_raw, ubi as usize).map_err(|e| anyhow::anyhow!("lz4: {e}"))?
        }
        other => anyhow::bail!("blocksinfo comp {other} unsupported"),
    };
    let mut br = Rd { d: &bi, p: 0 };
    br.skip(16);
    let nblocks = br.u32be();
    println!("  blocks: {nblocks}");
    for i in 0..nblocks {
        let u = br.u32be();
        let c = br.u32be();
        let f = br.u16be();
        println!(
            "    block[{i}] uncomp={u} comp={c} flags=0x{f:x} comp_type={}",
            f & 0x3f
        );
    }
    let nnodes = br.u32be();
    for _ in 0..nnodes {
        let off = br.i64be();
        let size = br.i64be();
        let nf = br.u32be();
        let np = br.cstr();
        println!("    node path={np} offset={off} size={size} flags=0x{nf:x}");
    }
    Ok(())
}

fn f3(v: Option<&abgen::value::Value>) -> (f64, f64, f64) {
    let g = |k: &str| {
        v.and_then(|x| x.get(k))
            .and_then(|x| x.as_f64())
            .unwrap_or(f64::NAN)
    };
    (g("x"), g("y"), g("z"))
}

fn main() -> Result<()> {
    for path in std::env::args().skip(1) {
        println!("== {path}");
        let data = std::fs::read(&path)?;
        container(&data)?;
        let bundle = Bundle::load_bytes(&data)?;
        for f in &bundle.files {
            let FileContent::Serialized(sf) = &f.content else {
                continue;
            };
            for obj in &sf.objects {
                let Ok(v) = sf.read_typetree(obj) else {
                    continue;
                };
                let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("");
                match obj.class_id {
                    43 => {
                        let aabb = v.get("m_LocalAABB");
                        let c = f3(aabb.and_then(|a| a.get("m_Center")));
                        let e = f3(aabb.and_then(|a| a.get("m_Extent")));
                        println!(
                            "  Mesh {name}: localAABB center=({:.4},{:.4},{:.4}) extent=({:.4},{:.4},{:.4})",
                            c.0, c.1, c.2, e.0, e.1, e.2
                        );
                        println!(
                            "    min=({:.4},{:.4},{:.4}) max=({:.4},{:.4},{:.4})",
                            c.0 - e.0,
                            c.1 - e.1,
                            c.2 - e.2,
                            c.0 + e.0,
                            c.1 + e.1,
                            c.2 + e.2
                        );
                        if let Some(ch) = v
                            .get("m_VertexData")
                            .and_then(|vd| vd.get("m_Channels"))
                            .and_then(|c| c.as_array())
                        {
                            let chans: Vec<String> = ch
                                .iter()
                                .enumerate()
                                .filter_map(|(i, c)| {
                                    let dim =
                                        c.get("dimension").and_then(|x| x.as_i64()).unwrap_or(0);
                                    if dim == 0 {
                                        return None;
                                    }
                                    Some(format!(
                                        "ch{i}(stream={} off={} fmt={} dim={})",
                                        c.get("stream").and_then(|x| x.as_i64()).unwrap_or(-1),
                                        c.get("offset").and_then(|x| x.as_i64()).unwrap_or(-1),
                                        c.get("format").and_then(|x| x.as_i64()).unwrap_or(-1),
                                        dim & 0xf
                                    ))
                                })
                                .collect();
                            println!("    channels: {}", chans.join(" "));
                        }
                        if let Some(sd) = v.get("m_StreamData") {
                            println!(
                                "    streamData path={:?} offset={:?} size={:?}",
                                sd.get("path").and_then(|x| x.as_str()).unwrap_or(""),
                                sd.get("offset").and_then(|x| x.as_i64()).unwrap_or(0),
                                sd.get("size").and_then(|x| x.as_i64()).unwrap_or(0)
                            );
                        }
                        let vd_size = v
                            .get("m_VertexData")
                            .and_then(|vd| vd.get("m_DataSize"))
                            .and_then(|x| x.as_bytes().map(|b| b.len() as i64))
                            .unwrap_or(-1);
                        println!("    vertexData inline bytes={vd_size}");
                    }
                    28 => {
                        let sz = v
                            .get("image data")
                            .and_then(|x| x.as_bytes().map(|b| b.len() as i64))
                            .unwrap_or(-1);
                        let sd = v.get("m_StreamData");
                        println!(
                            "  Texture2D {name}: inline_bytes={sz} streamData path={:?} offset={:?} size={:?}",
                            sd.and_then(|s| s.get("path")).and_then(|x| x.as_str()).unwrap_or(""),
                            sd.and_then(|s| s.get("offset")).and_then(|x| x.as_i64()).unwrap_or(0),
                            sd.and_then(|s| s.get("size")).and_then(|x| x.as_i64()).unwrap_or(0)
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
