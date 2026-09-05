use std::path::{Path, PathBuf};
use std::process::Command;

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
#[ignore = "needs chromium + DCL_ONE_SDK_TEST_NODE_MODULES; run scripts/creator-hub-ui-drive.sh"]
fn inspector_ui_edit_saves_composite_and_regenerates_crdt() {
    let node_modules = PathBuf::from(
        std::env::var("DCL_ONE_SDK_TEST_NODE_MODULES")
            .expect("set DCL_ONE_SDK_TEST_NODE_MODULES to a scene node_modules dir"),
    );
    assert!(node_modules.is_dir());

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("dcl-one-sdk-ui-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scene = root.join("scene");
    write(
        &scene.join("scene.json"),
        "{\"display\":{\"title\":\"UI E2E\"},\"main\":\"bin/index.js\",\"runtimeVersion\":\"7\",\"scene\":{\"parcels\":[\"0,0\"],\"base\":\"0,0\"}}",
    );
    write(
        &scene.join("tsconfig.json"),
        "{\n  \"compilerOptions\": { \"strict\": true },\n  \"include\": [\"src/**/*.ts\"],\n  \"extends\": \"@dcl/sdk/types/tsconfig.ecs7.json\"\n}",
    );
    write(&scene.join("src/index.ts"), "export function main() {}\n");
    write(&scene.join("package.json"), "{\"name\":\"ui-e2e\"}");
    let status = Command::new("cp")
        .arg("-al")
        .arg(&node_modules)
        .arg(scene.join("node_modules"))
        .status()
        .expect("cp -al node_modules");
    assert!(status.success());

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/creator-hub-ui-drive.sh");
    let evidence = root.join("evidence");
    let out = Command::new("bash")
        .arg(&script)
        .arg(&scene)
        .arg(&evidence)
        .env("DCL_ONE_SDK_BIN", env!("CARGO_BIN_EXE_dcl-one-sdk"))
        .output()
        .expect("running creator-hub-ui-drive.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "ui drive failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("RESULT {\"ok\":true"), "stdout: {stdout}");
    assert!(evidence.join("inspector-ui.png").is_file());
    assert!(evidence.join("ui-drive-summary.json").is_file());

    let _ = std::fs::remove_dir_all(&root);
}
