use crate::value::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalsPosition {
    Last,
    First,
}

impl ExternalsPosition {
    pub fn for_target(target: &str) -> Self {
        match target {
            // Linux64 + WebGL mirror windows/mac externals ordering.
            "windows" | "mac" | "linux" | "webgl" => ExternalsPosition::First,
            _ => ExternalsPosition::Last,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossBundlePosition {
    Last,
    AfterShader,
}

impl CrossBundlePosition {
    pub fn for_target(_target: &str) -> Self {
        CrossBundlePosition::Last
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obj {
    pub file_id: i64,
    pub path_id: i64,
}

impl Obj {
    pub const fn new(file_id: i64, path_id: i64) -> Self {
        Obj { file_id, path_id }
    }

    pub fn to_value(self) -> Value {
        crate::value::pptr(self.file_id, self.path_id)
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub guid: String,
    pub key: String,
    pub objects: Vec<Obj>,
    pub asset: Option<Obj>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerSlot {
    pub preload_index: usize,
    pub preload_size: usize,
    pub asset: Obj,
}

impl ContainerSlot {
    pub fn to_value(&self) -> Value {
        crate::map! {
            "preloadIndex" => self.preload_index as i64,
            "preloadSize" => self.preload_size as i64,
            "asset" => self.asset.to_value(),
        }
    }
}

fn guid_raw(guid_hex: &str) -> [u8; 16] {
    let h = guid_hex.as_bytes();
    let nib = |c: u8| (c as char).to_digit(16).expect("guid hex digit") as u8;
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = (nib(h[2 * i + 1]) << 4) | nib(h[2 * i]);
    }
    out
}

pub fn guid_sort_key(guid_hex: &str) -> [u32; 4] {
    let raw = guid_raw(guid_hex);
    let mut words = [0u32; 4];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u32::from_le_bytes([raw[4 * i], raw[4 * i + 1], raw[4 * i + 2], raw[4 * i + 3]]);
    }
    words
}

pub fn order_run_cab_merge<F>(objects: &[Obj], cab_for: F) -> Vec<Obj>
where
    F: Fn(i64) -> String,
{
    let mut out: Vec<Obj> = objects.to_vec();
    out.sort_by(|a, b| {
        cab_for(a.file_id)
            .cmp(&cab_for(b.file_id))
            .then_with(|| a.path_id.cmp(&b.path_id))
    });
    out
}

fn order_run_with(
    objects: &[Obj],
    pos: ExternalsPosition,
    cb_pos: CrossBundlePosition,
) -> Vec<Obj> {
    let mut internal: Vec<Obj> = objects.iter().copied().filter(|o| o.file_id == 0).collect();
    internal.sort_by_key(|o| o.path_id);

    let mut shader: Vec<Obj> = objects.iter().copied().filter(|o| o.file_id == 1).collect();
    shader.sort_by_key(|o| o.path_id);

    let mut cross_bundle: Vec<Obj> = objects.iter().copied().filter(|o| o.file_id >= 2).collect();
    cross_bundle.sort_by_key(|o| (o.file_id, o.path_id));

    let mut out: Vec<Obj> = Vec::with_capacity(objects.len());
    let (head, mid, tail) = match (pos, cb_pos) {
        (ExternalsPosition::First, CrossBundlePosition::Last) => (shader, internal, cross_bundle),
        (ExternalsPosition::First, CrossBundlePosition::AfterShader) => {
            (shader, cross_bundle, internal)
        }
        (ExternalsPosition::Last, CrossBundlePosition::Last) => (internal, shader, cross_bundle),
        (ExternalsPosition::Last, CrossBundlePosition::AfterShader) => {
            (internal, cross_bundle, shader)
        }
    };
    out.extend(head);
    out.extend(mid);
    out.extend(tail);
    out
}

pub fn build_preload_and_container(entries: &[Entry]) -> (Vec<Obj>, Vec<(String, ContainerSlot)>) {
    build_preload_and_container_per_entry(entries, |_| {
        (ExternalsPosition::Last, CrossBundlePosition::Last)
    })
}

pub fn build_preload_and_container_per_entry<F>(
    entries: &[Entry],
    pos_for: F,
) -> (Vec<Obj>, Vec<(String, ContainerSlot)>)
where
    F: Fn(&Entry) -> (ExternalsPosition, CrossBundlePosition),
{
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by_key(|e| guid_sort_key(&e.guid));

    let mut preload: Vec<Obj> = Vec::new();

    let mut index_by_key: Vec<(String, ContainerSlot)> = Vec::new();

    for e in &order {
        let (pos, cb_pos) = pos_for(e);
        let run = order_run_with(&e.objects, pos, cb_pos);
        let start = preload.len();
        let size = run.len();

        let asset = e.asset.or_else(|| run.first().copied()).unwrap_or(Obj {
            file_id: 0,
            path_id: 0,
        });

        preload.extend(run);

        let slot = ContainerSlot {
            preload_index: start,
            preload_size: size,
            asset,
        };
        match index_by_key.iter_mut().find(|(k, _)| *k == e.key) {
            Some((_, existing)) => *existing = slot,
            None => index_by_key.push((e.key.clone(), slot)),
        }
    }

    index_by_key.sort_by(|a, b| a.0.cmp(&b.0));
    (preload, index_by_key)
}

pub fn to_values(preload: &[Obj], container: &[(String, ContainerSlot)]) -> (Value, Value) {
    let preload_arr = Value::Array(preload.iter().map(|o| o.to_value()).collect());
    let container_arr = Value::Array(
        container
            .iter()
            .map(|(k, slot)| Value::Array(vec![Value::Str(k.clone()), slot.to_value()]))
            .collect(),
    );
    (preload_arr, container_arr)
}

pub fn empty_main_asset() -> Value {
    Value::Map(Map(vec![
        ("preloadIndex".into(), Value::Int(0)),
        ("preloadSize".into(), Value::Int(0)),
        ("asset".into(), crate::value::pptr(0, 0)),
    ]))
}
