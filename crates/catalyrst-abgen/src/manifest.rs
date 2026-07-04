use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_AB_VERSION: &str = "v0-abgen";

pub const DEFAULT_CONTENT_SERVER_URL: &str = "https://peer.decentraland.org/content";

pub const EXIT_CONVERSION_ERRORS_TOLERATED: i32 = 12;

pub fn exit_code_for_failures(failures: usize) -> i32 {
    if failures == 0 {
        0
    } else {
        EXIT_CONVERSION_ERRORS_TOLERATED
    }
}

#[allow(clippy::too_many_arguments)]
pub fn write_corpus_manifest(
    out_root: &Path,
    entity_id: &str,
    platform: &str,
    built: &[String],
    ab_version: &str,
    content_server_url: &str,
    exit_code: i32,
    date: &str,
) -> Result<PathBuf> {
    let mut files: Vec<String> = built.to_vec();
    files.sort();
    files.dedup();
    files.push("dcl".to_string());
    let manifest = serde_json::json!({
        "version": ab_version,
        "files": files,
        "exitCode": exit_code,
        "contentServerUrl": content_server_url,
        "date": date,
    });
    let dir = out_root.join(entity_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{platform}.manifest.json"));
    let text = serde_json::to_string_pretty(&manifest)?;
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn write_scene(
    out_dir: &str,
    entity_id: &str,
    platform: &str,
    bundles: &BTreeMap<String, Vec<u8>>,
    ab_version: &str,
    exit_code: i32,
    date: &str,
) -> Result<PathBuf> {
    let base = PathBuf::from(out_dir).join(entity_id);
    let pdir = base.join(platform);
    std::fs::create_dir_all(&pdir)?;

    for (fname, data) in bundles {
        std::fs::write(pdir.join(fname), data)?;
    }

    let files: Vec<serde_json::Value> = bundles
        .keys()
        .map(|k| serde_json::Value::String(k.clone()))
        .collect();

    let manifest = serde_json::json!({
        "version": ab_version,
        "files": files,
        "exitCode": exit_code,
        "date": date,
    });

    let text = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(base.join(format!("{platform}.manifest.json")), &text)?;
    Ok(base)
}

pub fn provenance(entity_id: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(entity_id.as_bytes());
    let inputs: String = h
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{inputs}+{}", env!("ABGEN_GIT_COMMIT"))
}

#[allow(dead_code)]
fn iso8601_utc_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let micros = dur.subsec_micros();

    let days = (total_secs / 86_400) as i64;
    let secs_of_day = (total_secs % 86_400) as i64;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{micros:06}+00:00")
}

const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATE: &str = "2026-01-02T03:04:05.000Z";

    #[test]
    fn corpus_manifest_shape_and_determinism() {
        let tmp =
            std::env::temp_dir().join(format!("abgen_corpus_manifest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        let built = vec![
            "QmB_deadbeef_windows".to_string(),
            "QmA_deadbeef_windows".to_string(),
            "QmB_deadbeef_windows".to_string(),
        ];
        let p = write_corpus_manifest(
            &tmp,
            "entityZ",
            "windows",
            &built,
            "v41",
            "http://cs",
            0,
            TEST_DATE,
        )
        .unwrap();
        assert_eq!(p, tmp.join("entityZ").join("windows.manifest.json"));
        let first = std::fs::read_to_string(&p).unwrap();
        let m: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(m["version"], "v41");
        assert_eq!(m["exitCode"], 0);
        assert_eq!(m["contentServerUrl"], "http://cs");
        assert_eq!(m["date"], TEST_DATE);
        assert_eq!(
            m["files"],
            serde_json::json!(["QmA_deadbeef_windows", "QmB_deadbeef_windows", "dcl"])
        );

        let second = std::fs::read_to_string(
            write_corpus_manifest(
                &tmp,
                "entityZ",
                "windows",
                &built,
                "v41",
                "http://cs",
                0,
                TEST_DATE,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn corpus_manifest_carries_nonzero_exit_code() {
        let tmp =
            std::env::temp_dir().join(format!("abgen_corpus_exitcode_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let built = vec!["QmA_deadbeef_windows".to_string()];
        let p = write_corpus_manifest(
            &tmp,
            "entityZ",
            "windows",
            &built,
            "v41",
            "http://cs",
            exit_code_for_failures(3),
            TEST_DATE,
        )
        .unwrap();
        let m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(m["exitCode"], EXIT_CONVERSION_ERRORS_TOLERATED);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn exit_code_mapping_matches_upstream_error_codes() {
        assert_eq!(exit_code_for_failures(0), 0);
        assert_eq!(exit_code_for_failures(1), 12);
        assert_eq!(exit_code_for_failures(352), 12);
        assert_eq!(EXIT_CONVERSION_ERRORS_TOLERATED, 12);
    }

    #[test]
    fn manifest_date_is_the_passed_build_scoped_date_not_provenance() {
        let tmp = std::env::temp_dir().join(format!("abgen_corpus_date_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let built: Vec<String> = Vec::new();
        let p = write_corpus_manifest(
            &tmp,
            "entityZ",
            "mac",
            &built,
            "v41",
            "http://cs",
            0,
            TEST_DATE,
        )
        .unwrap();
        let m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(m["date"], TEST_DATE);
        assert_ne!(m["date"], provenance("entityZ"));
        assert!(chrono::DateTime::parse_from_rfc3339(m["date"].as_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn civil_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));

        assert_eq!(civil_from_days(10957), (2000, 1, 1));

        let days_2026_05_20 = 20593;
        assert_eq!(civil_from_days(days_2026_05_20), (2026, 5, 20));
    }

    #[test]
    fn iso_format_shape() {
        let s = iso8601_utc_now();

        assert!(s.ends_with("+00:00"));
        assert_eq!(s.len(), "2026-05-20T12:00:00.000000+00:00".len());
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn writes_layout() {
        let tmp = std::env::temp_dir().join(format!("abgen_manifest_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mut bundles = BTreeMap::new();
        bundles.insert("b_windows".to_string(), b"data2".to_vec());
        bundles.insert("a_windows".to_string(), b"data1".to_vec());
        let base = write_scene(
            tmp.to_str().unwrap(),
            "entityX",
            "windows",
            &bundles,
            DEFAULT_AB_VERSION,
            0,
            TEST_DATE,
        )
        .unwrap();
        assert!(base.join("windows").join("a_windows").exists());
        let mtext = std::fs::read_to_string(base.join("windows.manifest.json")).unwrap();
        let m: serde_json::Value = serde_json::from_str(&mtext).unwrap();
        assert_eq!(m["version"], "v0-abgen");
        assert_eq!(m["exitCode"], 0);
        assert_eq!(m["date"], TEST_DATE);

        assert_eq!(m["files"][0], "a_windows");
        assert_eq!(m["files"][1], "b_windows");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
