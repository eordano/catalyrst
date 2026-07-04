use crate::unity::Bundle;
use anyhow::Result;

pub fn save_bundle(bundle: &Bundle) -> Result<Vec<u8>> {
    bundle.save_lz4()
}
