use crate::unity::bundle_file::ChunkMemo;
use crate::unity::Bundle;
use anyhow::Result;

pub fn save_bundle(bundle: &Bundle) -> Result<Vec<u8>> {
    bundle.save_lz4()
}

pub fn save_bundle_memo(bundle: &Bundle, memo: &mut ChunkMemo) -> Result<Vec<u8>> {
    bundle.save_lz4_memo(Some(memo))
}
