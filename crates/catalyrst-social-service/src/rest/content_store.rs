pub use catalyrst_storage::content_store::{
    is_valid_hash, ContentError, ContentStore, GcStats, HASH_HEX_LEN,
};

pub const MAX_BODY_BYTES: usize = 256 * 1024;
