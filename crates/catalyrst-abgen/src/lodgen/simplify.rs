use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const GLTFPACK_NIX_RECIPE: &str =
    "nix-shell -p meshoptimizer --run 'gltfpack -i <in.glb> -o <out.glb> -si 0.1 -noq'";

pub const SUBPROC_TIMEOUT_ENV: &str = "ABGEN_LOD_SUBPROC_TIMEOUT_S";

pub const SIMPLIFIER_ENV: &str = "ABGEN_SIMPLIFIER";

pub const DEFAULT_SIMPLIFIER: SimplifierBackend = SimplifierBackend::Meshopt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplifierBackend {
    Meshopt,
    Gltfpack,
}

impl SimplifierBackend {
    pub fn name(self) -> &'static str {
        match self {
            SimplifierBackend::Meshopt => "meshopt",
            SimplifierBackend::Gltfpack => "gltfpack",
        }
    }

    pub fn parse(s: &str) -> Result<SimplifierBackend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "meshopt" => Ok(SimplifierBackend::Meshopt),
            "gltfpack" => Ok(SimplifierBackend::Gltfpack),
            other => bail!("unknown simplifier {other:?} (want meshopt|gltfpack)"),
        }
    }

    pub fn from_env() -> SimplifierBackend {
        match std::env::var(SIMPLIFIER_ENV) {
            Ok(v) if !v.trim().is_empty() => match SimplifierBackend::parse(&v) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!(
                        "WARNING: {SIMPLIFIER_ENV}={v:?} ignored ({e:#}); using {}",
                        DEFAULT_SIMPLIFIER.name()
                    );
                    DEFAULT_SIMPLIFIER
                }
            },
            _ => DEFAULT_SIMPLIFIER,
        }
    }
}

pub fn subproc_deadline() -> Option<Duration> {
    let secs: u64 = std::env::var(SUBPROC_TIMEOUT_ENV)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if secs == 0 {
        None
    } else {
        Some(Duration::from_secs(secs))
    }
}

pub fn run_with_deadline(
    mut cmd: Command,
    deadline: Option<Duration>,
    label: &str,
) -> Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("spawn {label}"))?;
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let err_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().with_context(|| format!("wait {label}"))? {
            break st;
        }
        if let Some(d) = deadline {
            if started.elapsed() > d {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_h.join();
                let _ = err_h.join();
                bail!(
                    "{label} exceeded the {}s subprocess deadline ({SUBPROC_TIMEOUT_ENV}); killed",
                    d.as_secs()
                );
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = out_h.join().unwrap_or_default();
    let stderr = err_h.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[derive(Clone, Debug, Default)]
pub struct SimplifyReport {
    pub tris_before: usize,
    pub tris_after: usize,
    pub ratios_run: Vec<f64>,
    pub aggressive_final: bool,
    pub passthrough: bool,
    pub unsimplified: bool,
}

impl SimplifyReport {
    pub fn summary(&self) -> String {
        format!(
            "tris {} -> {} (ratios {:?}{}{}{})",
            self.tris_before,
            self.tris_after,
            self.ratios_run,
            if self.aggressive_final { ", -sa" } else { "" },
            if self.passthrough {
                ", passthrough"
            } else {
                ""
            },
            if self.unsimplified {
                ", UNSIMPLIFIED"
            } else {
                ""
            },
        )
    }
}

pub fn rescale_ratio(prev: f64, actual: u64, target: u64) -> f64 {
    if actual == 0 {
        return prev;
    }
    (prev * (target as f64 / actual as f64) * 0.9).clamp(1e-3, 1.0)
}

fn is_executable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_from(
    flag: Option<&Path>,
    env: Option<&std::ffi::OsStr>,
    path_var: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    if let Some(f) = flag {
        return Ok(f.to_path_buf());
    }
    if let Some(e) = env {
        if !e.is_empty() {
            return Ok(PathBuf::from(e));
        }
    }
    if let Some(pv) = path_var {
        for dir in std::env::split_paths(pv) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let cand = dir.join("gltfpack");
            if is_executable(&cand) {
                return Ok(cand);
            }
        }
    }
    bail!(
        "gltfpack not found (checked --gltfpack, ABGEN_GLTFPACK, PATH); install \
         meshoptimizer's gltfpack (1.1), e.g. {GLTFPACK_NIX_RECIPE}"
    )
}

pub fn resolve_gltfpack(flag: Option<&Path>) -> Result<PathBuf> {
    let env = std::env::var_os("ABGEN_GLTFPACK");
    let path_var = std::env::var_os("PATH");
    resolve_from(flag, env.as_deref(), path_var.as_deref())
}

fn run_gltfpack(
    gltfpack: &Path,
    input: &Path,
    output: &Path,
    ratio: f64,
    aggressive: bool,
    error_limit: Option<f64>,
) -> Result<()> {
    let mut cmd = Command::new(gltfpack);
    cmd.arg("-i").arg(input).arg("-o").arg(output);
    cmd.arg("-si").arg(format!("{ratio}"));
    cmd.arg("-sp");
    if aggressive {
        cmd.arg("-sa");
    }
    if let Some(se) = error_limit {
        cmd.arg("-se").arg(format!("{se}"));
    }
    cmd.arg("-noq");
    let out = run_with_deadline(
        cmd,
        subproc_deadline(),
        &format!("gltfpack -si {ratio} ({})", gltfpack.display()),
    )
    .with_context(|| {
        format!(
            "run {} (if missing: {GLTFPACK_NIX_RECIPE})",
            gltfpack.display()
        )
    })?;
    if !out.status.success() {
        bail!(
            "gltfpack -si {ratio}{} failed ({}): {}{}",
            if aggressive { " -sa" } else { "" },
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

fn glb_tris(path: &Path) -> Result<usize> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let model = super::model::from_glb_bytes(&bytes, "simplify-check")
        .with_context(|| format!("reparse {}", path.display()))?;
    Ok(model.total_tris())
}

fn copy_through(input: &Path, output: &Path) -> Result<()> {
    if input != output {
        std::fs::copy(input, output)
            .with_context(|| format!("copy {} -> {}", input.display(), output.display()))?;
    }
    Ok(())
}

pub fn passthrough(input: &Path, output: &Path) -> Result<SimplifyReport> {
    let tris = glb_tris(input)?;
    copy_through(input, output)?;
    Ok(SimplifyReport {
        tris_before: tris,
        tris_after: tris,
        ratios_run: Vec::new(),
        aggressive_final: false,
        passthrough: true,
        unsimplified: false,
    })
}

pub fn copy_unsimplified(input: &Path, output: &Path) -> Result<SimplifyReport> {
    let mut report = passthrough(input, output)?;
    report.unsimplified = true;
    eprintln!(
        "WARNING: --allow-unsimplified: {} copied through VERBATIM ({} tris, no decimation); \
         this is a completeness escape hatch, not a production mode",
        input.display(),
        report.tris_before
    );
    Ok(report)
}

pub fn simplify(
    input: &Path,
    output: &Path,
    ratio: f64,
    tri_cap: Option<u64>,
    gltfpack: &Path,
) -> Result<SimplifyReport> {
    let tris_before = glb_tris(input)?;
    let under_cap = |tris: usize| tri_cap.is_none_or(|c| tris as u64 <= c);
    if ratio >= 1.0 && under_cap(tris_before) {
        copy_through(input, output)?;
        return Ok(SimplifyReport {
            tris_before,
            tris_after: tris_before,
            ratios_run: Vec::new(),
            aggressive_final: false,
            passthrough: true,
            unsimplified: false,
        });
    }
    let mut report = SimplifyReport {
        tris_before,
        ..Default::default()
    };
    let mut current = ratio.clamp(1e-3, 1.0);
    let mut tris_after = tris_before;
    for attempt in 0..4 {
        let aggressive = attempt == 3;
        run_gltfpack(gltfpack, input, output, current, aggressive, None)?;
        report.ratios_run.push(current);
        report.aggressive_final = aggressive;
        tris_after = glb_tris(output)?;
        if under_cap(tris_after) || aggressive {
            break;
        }
        current = if attempt == 2 {
            rescale_ratio(1.0, tris_before as u64, tri_cap.unwrap_or(1))
        } else {
            rescale_ratio(current, tris_after as u64, tri_cap.unwrap_or(1))
        };
    }
    let mut error_relaxed = false;
    if let Some(cap) = tri_cap {
        if tris_after as u64 > cap {
            // -sa alone floors at the default 1% -se error bound on multi-component
            // bakes; relaxing -se lets the triangle-count target dominate.
            error_relaxed = true;
            let ratio_sa = rescale_ratio(1.0, tris_before as u64, cap.max(1));
            let (mut lo, mut hi) = (0.01f64, 1.0f64);
            let mut best: Option<(usize, Vec<u8>)> = None;
            for _ in 0..6 {
                let se = (lo + hi) / 2.0;
                run_gltfpack(gltfpack, input, output, ratio_sa, true, Some(se))?;
                report.ratios_run.push(ratio_sa);
                report.aggressive_final = true;
                let t = glb_tris(output)?;
                if t as u64 <= cap {
                    hi = se;
                    if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                        best = Some((t, std::fs::read(output)?));
                    }
                } else {
                    lo = se;
                }
            }
            match best {
                Some((t, bytes)) => {
                    std::fs::write(output, &bytes)?;
                    tris_after = t;
                }
                None => bail!(
                    "tri cap {cap} not reached after {} gltfpack attempts (final {} tris, ratios {:?})",
                    report.ratios_run.len(),
                    tris_after,
                    report.ratios_run
                ),
            }
        }
    }
    if let Some(cap) = tri_cap.filter(|_| !error_relaxed) {
        let floor = (cap as f64 * 0.8) as usize;
        if tris_after > 0
            && (tris_before as u64) > cap
            && (tris_after as u64) <= cap
            && tris_after < floor
        {
            let mut lo_ratio = report.ratios_run.last().copied().unwrap_or(current);
            let mut hi_ratio: Option<f64> = None;
            let mut best = std::fs::read(output)?;
            for _ in 0..6 {
                if tris_after >= floor {
                    break;
                }
                let cand = match hi_ratio {
                    None => (cap as f64 * 0.9 / tris_before as f64)
                        .max(lo_ratio * 2.0)
                        .clamp(1e-3, 1.0),
                    Some(h) => (lo_ratio + h) / 2.0,
                };
                if (cand - lo_ratio).abs() < 1e-6 {
                    break;
                }
                run_gltfpack(gltfpack, input, output, cand, true, None)?;
                let t = glb_tris(output)?;
                if t as u64 > cap {
                    hi_ratio = Some(cand);
                } else {
                    lo_ratio = cand;
                    if t > tris_after {
                        tris_after = t;
                        best = std::fs::read(output)?;
                        report.ratios_run.push(cand);
                    }
                }
            }
            std::fs::write(output, &best)?;
        }
    }
    report.tris_after = tris_after;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lodgen::emit::emit_glb;
    use crate::lodgen::model::{AlphaClass, LodMaterial, LodModel, LodPrimitive};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "abgen-lod-simplify-test-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn grid_glb(n: u32) -> Vec<u8> {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                let x = i as f32 / n as f32;
                let z = j as f32 / n as f32;
                let y = 0.05
                    * ((x * 12.0).sin() + (z * 12.0).cos())
                    * (1.0 + 0.3 * ((x * 5.0 + z * 7.0).sin()));
                positions.push([x * 10.0, y, z * 10.0]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([x, z]);
            }
        }
        let mut indices = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let a = j * (n + 1) + i;
                let b = a + 1;
                let c = a + n + 1;
                let d = c + 1;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        emit_glb(&LodModel {
            root_name: "grid".to_string(),
            primitives: vec![LodPrimitive {
                positions,
                normals,
                uvs,
                indices,
                material: 0,
                ..Default::default()
            }],
            materials: vec![LodMaterial {
                name: "m".to_string(),
                class: AlphaClass::Opaque,
                base_color: [1.0, 1.0, 1.0, 1.0],
                cutoff: 0.5,
                image: None,
                double_sided: false,
            }],
            images: Vec::new(),
            log: Vec::new(),
        })
        .unwrap()
    }

    #[cfg(unix)]
    fn fake_gltfpack(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("gltfpack");
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn resolution_order_flag_env_path() {
        let dir = temp_dir("resolve");
        let on_path = fake_gltfpack(&dir);
        let path_var = std::ffi::OsString::from(dir.as_os_str());

        let flag = Path::new("/from/flag/gltfpack");
        let env = std::ffi::OsString::from("/from/env/gltfpack");
        assert_eq!(
            resolve_from(Some(flag), Some(&env), Some(&path_var)).unwrap(),
            flag
        );
        assert_eq!(
            resolve_from(None, Some(&env), Some(&path_var)).unwrap(),
            PathBuf::from("/from/env/gltfpack")
        );
        assert_eq!(resolve_from(None, None, Some(&path_var)).unwrap(), on_path);
        let empty_env = std::ffi::OsString::new();
        assert_eq!(
            resolve_from(None, Some(&empty_env), Some(&path_var)).unwrap(),
            on_path
        );

        let plain = dir.join("sub");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("gltfpack"), "not executable").unwrap();
        let miss_var = std::ffi::OsString::from(plain.as_os_str());
        let err = resolve_from(None, None, Some(&miss_var)).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("meshoptimizer"), "{msg}");
        assert!(msg.contains("nix-shell"), "{msg}");
    }

    #[test]
    fn rescale_ratio_scales_by_target_over_actual() {
        let r = rescale_ratio(0.1, 1000, 500);
        assert!((r - 0.045).abs() < 1e-12, "{r}");
        let r = rescale_ratio(1.0, 2048, 100);
        assert!((r - 0.0439453125).abs() < 1e-9, "{r}");
        assert_eq!(rescale_ratio(0.001, 10_000_000, 1), 1e-3);
        assert_eq!(rescale_ratio(0.5, 100, 100_000), 1.0);
        assert_eq!(rescale_ratio(0.25, 0, 500), 0.25);
    }

    #[test]
    fn ratio_one_under_cap_is_byte_passthrough() {
        let dir = temp_dir("passthrough");
        let glb = grid_glb(8);
        let input = dir.join("in.glb");
        let output = dir.join("out.glb");
        std::fs::write(&input, &glb).unwrap();
        let report = simplify(
            &input,
            &output,
            1.0,
            Some(1_000_000),
            Path::new("/nonexistent/gltfpack"),
        )
        .unwrap();
        assert!(report.passthrough);
        assert!(!report.unsimplified);
        assert!(report.ratios_run.is_empty());
        assert_eq!(report.tris_before, 128);
        assert_eq!(report.tris_after, 128);
        assert_eq!(std::fs::read(&output).unwrap(), glb);

        let report = simplify(
            &input,
            &output,
            1.0,
            None,
            Path::new("/nonexistent/gltfpack"),
        )
        .unwrap();
        assert!(report.passthrough);
    }

    #[test]
    fn passthrough_copies_byte_identical_without_gltfpack() {
        let dir = temp_dir("purepass");
        let glb = grid_glb(6);
        let input = dir.join("in.glb");
        let output = dir.join("out.glb");
        std::fs::write(&input, &glb).unwrap();
        let report = passthrough(&input, &output).unwrap();
        assert!(report.passthrough);
        assert!(!report.unsimplified);
        assert!(report.ratios_run.is_empty());
        assert_eq!(report.tris_before, report.tris_after);
        assert_eq!(report.tris_before, 72);
        assert_eq!(std::fs::read(&output).unwrap(), glb);
        assert!(!report.summary().contains("UNSIMPLIFIED"));
    }

    #[test]
    fn allow_unsimplified_copies_verbatim() {
        let dir = temp_dir("unsimplified");
        let glb = grid_glb(4);
        let input = dir.join("in.glb");
        let output = dir.join("out.glb");
        std::fs::write(&input, &glb).unwrap();
        let report = copy_unsimplified(&input, &output).unwrap();
        assert!(report.unsimplified);
        assert!(report.passthrough);
        assert_eq!(report.tris_before, report.tris_after);
        assert_eq!(report.tris_before, 32);
        assert_eq!(std::fs::read(&output).unwrap(), glb);
        assert!(report.summary().contains("UNSIMPLIFIED"));
    }

    #[test]
    fn gltfpack_reduces_grid_and_output_reparses() {
        let Ok(bin) = resolve_gltfpack(None) else {
            eprintln!("SKIP: gltfpack not resolvable ({GLTFPACK_NIX_RECIPE})");
            return;
        };
        let dir = temp_dir("reduce");
        let glb = grid_glb(32);
        let input = dir.join("in.glb");
        let output = dir.join("out.glb");
        std::fs::write(&input, &glb).unwrap();

        let report = simplify(&input, &output, 0.25, None, &bin).unwrap();
        assert_eq!(report.tris_before, 2048);
        assert!(report.tris_after > 0);
        assert!(
            report.tris_after < report.tris_before,
            "{} !< {}",
            report.tris_after,
            report.tris_before
        );
        assert_eq!(report.ratios_run, vec![0.25]);

        let capped = simplify(&input, &output, 1.0, Some(100), &bin).unwrap();
        assert!(capped.tris_after <= 100, "{}", capped.tris_after);
        assert!(capped.tris_after > 0);
        assert!(capped.ratios_run.len() >= 2, "{:?}", capped.ratios_run);
    }

    #[cfg(unix)]
    #[test]
    fn deadline_kills_hung_subprocess_within_budget() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let t = std::time::Instant::now();
        let err = run_with_deadline(cmd, Some(Duration::from_millis(300)), "sleep 30").unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "kill took {:?}",
            t.elapsed()
        );
        let msg = format!("{err:#}");
        assert!(msg.contains("deadline"), "{msg}");
        assert!(msg.contains(SUBPROC_TIMEOUT_ENV), "{msg}");
    }

    #[cfg(unix)]
    #[test]
    fn deadline_passthrough_captures_output() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out-line; echo err-line >&2; exit 0"]);
        let out = run_with_deadline(cmd, Some(Duration::from_secs(30)), "sh echo").unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out-line");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err-line");

        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 3"]);
        let out = run_with_deadline(cmd, None, "sh exit 3").unwrap();
        assert!(!out.status.success());
    }
}
