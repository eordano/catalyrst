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
            if let FileContent::Serialized(sf) = &f.content {
                for obj in &sf.objects {
                    if obj.class_id != 21 {
                        continue;
                    }
                    match sf.read_typetree(obj) {
                        Ok(v) => println!("{v:#?}"),
                        Err(e) => println!("  typetree error: {e:#}"),
                    }
                }
            }
        }
    }
}
