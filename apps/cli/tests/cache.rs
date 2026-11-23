use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tomet"))
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_cache_entry(base_dir: &Path, kind: &str, key: &str, value_cotomet: &str) {
    let path = base_dir.join(tomet_cache::entry_rel_path(kind, key));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!("::kind(cache)\n\nkey: {key:?}\nvalue: {value_cotomet}\n"),
    )
    .unwrap();
}

#[test]
fn check_fails_on_cache_miss_and_passes_when_cache_exists() {
    let root = std::env::temp_dir().join(format!("tomet_cli_cache_{}", nanoid::nanoid!()));
    let _guard = TempDirGuard(root.clone());
    fs::create_dir_all(&root).unwrap();

    fs::write(
        root.join("vault.config.tmt"),
        "::kind(config)\n::cache(dir: \".tomet/cache\", local: true)\n",
    )
    .unwrap();
    fs::write(
        root.join("readme.tmt"),
        "::export(type: commonmark, path: \"README.md\")\n\nRev: $cache.command(\"git rev-parse --short HEAD\")\nUser: $cache.command(\"whoami\", local: true)\n",
    )
    .unwrap();

    let local_cache_dir = root.join("external-local-cache");

    // 1. Cache miss -> `tomet check` exits non-zero (1) and reports errors.
    let miss_check = Command::new(bin_path())
        .arg("check")
        .arg(&root)
        .env("TOMET_CACHE_DIR", &local_cache_dir)
        .output()
        .unwrap();
    assert!(
        !miss_check.status.success(),
        "expected non-zero exit on cache miss, stdout={}, stderr={}",
        String::from_utf8_lossy(&miss_check.stdout),
        String::from_utf8_lossy(&miss_check.stderr)
    );
    let stderr = String::from_utf8_lossy(&miss_check.stderr);
    assert!(stderr.contains("cache miss"), "{stderr}");
    assert!(
        !stderr.contains("warning:"),
        "cache miss must be an error, not a warning: {stderr}"
    );

    // 2. Cache miss -> `tomet to-md` leaves original expression text in output.
    let miss_md = Command::new(bin_path())
        .arg("to-md")
        .arg(root.join("readme.tmt"))
        .env("TOMET_CACHE_DIR", &local_cache_dir)
        .output()
        .unwrap();
    assert!(miss_md.status.success());
    let miss_stdout = String::from_utf8_lossy(&miss_md.stdout);
    assert!(
        miss_stdout.contains("$cache.command(\"git rev-parse --short HEAD\")"),
        "{miss_stdout}"
    );

    // 3. Populate both shared and local `.cache.ctmt` files.
    write_cache_entry(
        &root.join(".tomet/cache"),
        "command",
        "git rev-parse --short HEAD",
        "\"deadbeef\"",
    );
    write_cache_entry(&local_cache_dir, "command", "whoami", "\"tefla\"");

    // 4. `tomet check` now passes with exit code 0 (and cache dir itself is excluded from scanning).
    let hit_check = Command::new(bin_path())
        .arg("check")
        .arg(&root)
        .env("TOMET_CACHE_DIR", &local_cache_dir)
        .output()
        .unwrap();
    assert!(
        hit_check.status.success(),
        "expected exit 0 when cache exists, stdout={}, stderr={}",
        String::from_utf8_lossy(&hit_check.stdout),
        String::from_utf8_lossy(&hit_check.stderr)
    );

    // 5. `tomet to-md` expands both cached values.
    let hit_md = Command::new(bin_path())
        .arg("to-md")
        .arg(root.join("readme.tmt"))
        .env("TOMET_CACHE_DIR", &local_cache_dir)
        .output()
        .unwrap();
    assert!(hit_md.status.success());
    let hit_stdout = String::from_utf8_lossy(&hit_md.stdout);
    assert!(hit_stdout.contains("Rev: deadbeef"), "{hit_stdout}");
    assert!(hit_stdout.contains("User: tefla"), "{hit_stdout}");

    // 6. `tomet export` writes the expanded cached values to README.md.
    let export_out = Command::new(bin_path())
        .arg("export")
        .arg(&root)
        .env("TOMET_CACHE_DIR", &local_cache_dir)
        .output()
        .unwrap();
    assert!(
        export_out.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );
    let exported_md = fs::read_to_string(root.join("README.md")).unwrap();
    assert!(exported_md.contains("Rev: deadbeef"), "{exported_md}");
    assert!(exported_md.contains("User: tefla"), "{exported_md}");
}
