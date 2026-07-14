use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) fn tmp_sibling(dst: &Path) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut os = dst.as_os_str().to_owned();
    os.push(format!(".tmp.{}.{}", std::process::id(), seq));
    PathBuf::from(os)
}
