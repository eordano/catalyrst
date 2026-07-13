use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

use super::{ISS_MANIFEST_BASE, ISS_SUFFIX, MANIFEST_OUTPUT_DIR, MANIFEST_SUFFIX};

pub fn fetch_iss(scene_id: &str) -> Result<Option<Vec<u8>>> {
    let url = format!("{ISS_MANIFEST_BASE}/{scene_id}{ISS_SUFFIX}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into();
    match agent
        .get(&url)
        .header("User-Agent", crate::catalyst::UA)
        .call()
    {
        Ok(resp) => {
            let mut buf = Vec::new();
            use std::io::Read;
            resp.into_body().into_reader().read_to_end(&mut buf)?;
            Ok(Some(buf))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(anyhow!("GET {url}: {e}")),
    }
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("mkdir {}", dst.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("read dir {}", src.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s == "node_modules" || name_s == "dist" || name_s == ".git" {
            continue;
        }
        let sp = entry.path();
        let dp = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&sp, &dp)?;
        } else {
            std::fs::copy(&sp, &dp)
                .with_context(|| format!("copy {} -> {}", sp.display(), dp.display()))?;
        }
    }
    Ok(())
}

fn run_npm(work_dir: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = std::process::Command::new("npm");
    cmd.args(args).current_dir(work_dir);
    let out = crate::lodgen::simplify::run_with_deadline(
        cmd,
        crate::lodgen::simplify::subproc_deadline(),
        &format!("npm {args:?} in {}", work_dir.display()),
    )?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        bail!(
            "npm {:?} in {} failed ({}):\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args,
            work_dir.display(),
            out.status,
            stdout,
            stderr
        );
    }
    Ok(stdout)
}

pub const MANIFEST_BUILDER_DONE_MARKER: &str = "Finished running frames!";

static MANIFEST_BUILDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn run_manifest_builder(
    coords: &str,
    tool_dir: &Path,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    let _serialized = MANIFEST_BUILDER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let src_pkg_path = tool_dir.join("package.json");
    if !src_pkg_path.is_file() {
        bail!(
            "manifest-builder tool dir {} has no package.json",
            tool_dir.display()
        );
    }
    let src_pkg = std::fs::read(&src_pkg_path)?;
    let installed = match std::fs::read(work_dir.join("package.json")) {
        Ok(b) => {
            b == src_pkg && work_dir.join("node_modules").is_dir() && work_dir.join("dist").is_dir()
        }
        Err(_) => false,
    };
    if !installed {
        copy_tree(tool_dir, work_dir)?;
        run_npm(work_dir, &["ci", "--ignore-scripts"])?;
        run_npm(work_dir, &["run", "build"])?;
    }
    let coords_arg = format!("--coords={coords}");
    let stdout = run_npm(work_dir, &["run", "start", &coords_arg, "--overwrite"])?;
    let out_dir = work_dir.join(MANIFEST_OUTPUT_DIR);
    if let Some(rest) = stdout.split("scene id:").nth(1) {
        let scene_id: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ';')
            .collect();
        if !scene_id.is_empty() {
            let candidate = out_dir.join(format!("{scene_id}{MANIFEST_SUFFIX}"));
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    if out_dir.is_dir() {
        for entry in std::fs::read_dir(&out_dir)
            .with_context(|| format!("read manifest output dir {}", out_dir.display()))?
        {
            let entry = entry?;
            let p = entry.path();
            if !p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(MANIFEST_SUFFIX))
            {
                continue;
            }
            let mtime = entry.metadata()?.modified()?;
            if newest.as_ref().is_none_or(|(t, _)| mtime > *t) {
                newest = Some((mtime, p));
            }
        }
    }
    match newest {
        Some((_, p)) => Ok(Some(p)),
        None if stdout.contains(MANIFEST_BUILDER_DONE_MARKER) => Ok(None),
        None => bail!(
            "manifest builder produced no {MANIFEST_SUFFIX} file in {}:\n{}",
            out_dir.display(),
            stdout
        ),
    }
}
