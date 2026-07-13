use abgen::unity::bundle_file::{Bundle, FileContent};

fn main() {
    for path in std::env::args().skip(1) {
        println!("== {path}");
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("  read error: {e}");
                continue;
            }
        };
        let bundle = match Bundle::load_bytes(&data) {
            Ok(b) => b,
            Err(e) => {
                println!("  parse error: {e:#}");
                continue;
            }
        };
        for f in &bundle.files {
            match &f.content {
                FileContent::Raw(bytes) => {
                    println!("  raw file {} bytes={}", f.name, bytes.len());
                }
                FileContent::Serialized(sf) => {
                    println!("  serialized file {} objects={}", f.name, sf.objects.len());
                    for obj in &sf.objects {
                        match sf.read_typetree(obj) {
                            Ok(v) => {
                                let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("");
                                let mut extra = String::new();
                                if obj.class_id == 49 {
                                    if let Some(s) = v.get("m_Script") {
                                        if let Some(txt) = s.as_str() {
                                            extra = format!(
                                                " script_len={} script_head={:?}",
                                                txt.len(),
                                                &txt[..txt.len().min(80)]
                                            );
                                        } else if let Some(b) = s.as_bytes() {
                                            extra = format!(
                                                " script_len={} script_head={:?}",
                                                b.len(),
                                                &b[..b.len().min(80)]
                                            );
                                        }
                                    }
                                }
                                if obj.class_id == 142 {
                                    extra = format!(
                                        " deps={:?} container_keys={:?}",
                                        v.get("m_Dependencies"),
                                        v.get("m_Container").map(|c| c
                                            .as_array()
                                            .map(|a| a
                                                .iter()
                                                .filter_map(|kv| kv
                                                    .as_array()
                                                    .and_then(|p| p.first())
                                                    .and_then(|k| k.as_str())
                                                    .map(String::from))
                                                .collect::<Vec<_>>())
                                            .unwrap_or_default())
                                    );
                                }
                                println!(
                                    "    class={} pid={} name={name}{extra}",
                                    obj.class_id, obj.path_id
                                );
                            }
                            Err(e) => println!(
                                "    class={} pid={} <typetree error: {e}>",
                                obj.class_id, obj.path_id
                            ),
                        }
                    }
                }
            }
        }
    }
}
