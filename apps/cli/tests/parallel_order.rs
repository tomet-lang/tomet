use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tomet"))
}

#[cfg(unix)]
fn is_running_as_root() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() == 0 }
}

#[cfg(not(unix))]
fn is_running_as_root() -> bool {
    false
}

struct TempVaultGuard {
    path: PathBuf,
    unreadable_files: Vec<PathBuf>,
}

impl TempVaultGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            unreadable_files: Vec::new(),
        }
    }

    fn add_unreadable(&mut self, file: PathBuf) {
        self.unreadable_files.push(file);
    }
}

impl Drop for TempVaultGuard {
    fn drop(&mut self) {
        for file in &self.unreadable_files {
            if file.exists() {
                if let Ok(meta) = fs::metadata(file) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o644);
                    let _ = fs::set_permissions(file, perms);
                }
            }
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn setup_vault(guard: &mut TempVaultGuard) -> bool {
    let root = guard.path.clone();
    fs::create_dir_all(&root).unwrap();

    let running_root = is_running_as_root();
    if running_root {
        eprintln!("Skipping unreadable files test branch: running as root (chmod 000 is ignored)");
    }

    for suffix in ["a", "b", "c"] {
        fs::write(
            root.join(format!("01-clean-{suffix}.tmt")),
            format!("# Clean Document {suffix}\n\nThis is a clean document with no errors.\n"),
        )
        .unwrap();

        fs::write(
            root.join(format!("02-parse-err-{suffix}.tmt")),
            format!("# Parse Error {suffix}\n\n@unclosed_{suffix}{{\n"),
        )
        .unwrap();

        fs::write(
            root.join(format!("03-val-err-{suffix}.tmt")),
            format!(
                "@section(id:\"entries_{suffix}\")[\n  @heading[ Disallowed {suffix} ]\n]:rule(allow:list(card))\n"
            ),
        )
        .unwrap();

        fs::write(
            root.join(format!("04-warn-unresolved-{suffix}.tmt")),
            format!("# Warning Document {suffix}\n\nHere is an unresolved expression: ${{unknown_id_{suffix}}}.\n"),
        )
        .unwrap();

        fs::write(
            root.join(format!("05-unformatted-{suffix}.tmt")),
            format!("a_{suffix}  \n\n\n\nb_{suffix}  \n"),
        )
        .unwrap();

        if !running_root {
            let unreadable = root.join(format!("06-unreadable-{suffix}.tmt"));
            fs::write(&unreadable, "").unwrap();
            let mut perms = fs::metadata(&unreadable).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&unreadable, perms).unwrap();
            guard.add_unreadable(unreadable);
        }
    }

    !running_root
}

fn copy_vault(src: &Path, dst_guard: &mut TempVaultGuard, has_unreadable: bool) {
    fs::create_dir_all(&dst_guard.path).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let dest = dst_guard.path.join(&name);
        if name_str.starts_with("06-unreadable") && has_unreadable {
            fs::write(&dest, "").unwrap();
            let mut perms = fs::metadata(&dest).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&dest, perms).unwrap();
            dst_guard.add_unreadable(dest);
        } else if path.is_file() {
            fs::copy(&path, &dest).unwrap();
        }
    }
}

struct RunOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_cmd(bin: &Path, args: &[&str]) -> RunOutput {
    let output = Command::new(bin)
        .args(args)
        .output()
        .expect("failed to execute tomet binary");
    RunOutput {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn assert_deterministic_50_runs(args_prefix: &[&str], vault_dir: &Path) {
    let bin = bin_path();
    let mut full_args = args_prefix.to_vec();
    let vault_str = vault_dir.to_str().unwrap();
    full_args.push(vault_str);

    let baseline = run_cmd(&bin, &full_args);

    for run_i in 1..50 {
        let current = run_cmd(&bin, &full_args);
        assert_eq!(
            baseline.status.code(),
            current.status.code(),
            "Exit code differed on run {run_i} for args: {full_args:?}"
        );
        assert_eq!(
            baseline.stdout, current.stdout,
            "stdout differed on run {run_i} for args: {full_args:?}"
        );
        assert_eq!(
            baseline.stderr, current.stderr,
            "stderr differed on run {run_i} for args: {full_args:?}"
        );
    }
}

#[test]
fn test_parallel_determinism_and_order() {
    let temp_root = std::env::temp_dir().join(format!(
        "tomet_parallel_test_{}_{}",
        std::process::id(),
        nanoid::nanoid!()
    ));

    let mut guard = TempVaultGuard::new(temp_root);
    let has_unreadable = setup_vault(&mut guard);
    let vault_dir = &guard.path;
    let vault_str = vault_dir.to_str().unwrap();
    let bin = bin_path();

    let total_files = if has_unreadable { 18 } else { 15 };
    let failed_files = if has_unreadable { 9 } else { 6 };

    // 1. Exact expected output for `check` (text)
    let check_out = run_cmd(&bin, &["check", vault_str]);
    assert_eq!(check_out.status.code(), Some(1));
    let stderr_text = String::from_utf8_lossy(&check_out.stderr);
    let stderr_lines: Vec<&str> = stderr_text.lines().collect();

    // Verify ordering of branches and their a, b, c files
    let mut pos = 0;
    for suffix in ["a", "b", "c"] {
        let expected_file = format!("02-parse-err-{suffix}.tmt");
        let found = stderr_lines[pos..]
            .iter()
            .position(|l| l.contains(&expected_file));
        assert!(
            found.is_some(),
            "Expected parse error for {expected_file} after line {pos}; stderr:\n{stderr_text}"
        );
        pos += found.unwrap();
    }
    for suffix in ["a", "b", "c"] {
        let expected_file = format!("03-val-err-{suffix}.tmt");
        let found = stderr_lines[pos..]
            .iter()
            .position(|l| l.contains(&expected_file));
        assert!(
            found.is_some(),
            "Expected val error for {expected_file} after line {pos}; stderr:\n{stderr_text}"
        );
        pos += found.unwrap();
    }
    for suffix in ["a", "b", "c"] {
        let expected_file = format!("04-warn-unresolved-{suffix}.tmt");
        let found = stderr_lines[pos..]
            .iter()
            .position(|l| l.contains(&expected_file));
        assert!(
            found.is_some(),
            "Expected warning for {expected_file} after line {pos}; stderr:\n{stderr_text}"
        );
        pos += found.unwrap();
    }
    if has_unreadable {
        for suffix in ["a", "b", "c"] {
            let expected_line = format!(
                "could not read {}",
                vault_dir
                    .join(format!("06-unreadable-{suffix}.tmt"))
                    .display()
            );
            let found = stderr_lines[pos..].iter().position(|l| *l == expected_line);
            assert!(
                found.is_some(),
                "Expected unreadable line `{expected_line}` after line {pos}; stderr:\n{stderr_text}"
            );
            pos += found.unwrap();
        }
    }
    assert_eq!(
        *stderr_lines.last().expect("stderr should have summary"),
        format!("{failed_files} of {total_files} file(s) failed")
    );

    // 2. Exact expected output for `check --json`
    let check_json_out = run_cmd(&bin, &["check", "--json", vault_str]);
    assert_eq!(check_json_out.status.code(), Some(1));
    let check_json: serde_json::Value =
        serde_json::from_slice(&check_json_out.stdout).expect("valid json from check --json");
    let check_entries = check_json.as_array().expect("check --json returns array");

    let expected_files = [
        "02-parse-err-a.tmt",
        "02-parse-err-b.tmt",
        "02-parse-err-c.tmt",
        "03-val-err-a.tmt",
        "03-val-err-b.tmt",
        "03-val-err-c.tmt",
        "04-warn-unresolved-a.tmt",
        "04-warn-unresolved-b.tmt",
        "04-warn-unresolved-c.tmt",
    ];
    assert_eq!(
        check_entries.len(),
        expected_files.len(),
        "Unexpected number of check --json entries"
    );
    for (entry, expected_name) in check_entries.iter().zip(expected_files) {
        let file_path = entry["file"].as_str().expect("file string");
        assert!(
            file_path.ends_with(expected_name),
            "Expected {expected_name}, got {file_path}"
        );
    }

    // 3. Exact expected output for `stats --json`
    let stats_json_out = run_cmd(&bin, &["stats", "--json", vault_str]);
    assert_eq!(stats_json_out.status.code(), Some(1));
    let stats_json: serde_json::Value =
        serde_json::from_slice(&stats_json_out.stdout).expect("valid json from stats --json");
    let stats_errors = stats_json["errors"].as_array().expect("errors array");

    let mut expected_stats_errors = vec![
        "02-parse-err-a.tmt",
        "02-parse-err-b.tmt",
        "02-parse-err-c.tmt",
    ];
    if has_unreadable {
        expected_stats_errors.extend(&[
            "06-unreadable-a.tmt",
            "06-unreadable-b.tmt",
            "06-unreadable-c.tmt",
        ]);
    }
    assert_eq!(
        stats_errors.len(),
        expected_stats_errors.len(),
        "Unexpected number of stats --json errors"
    );
    for (err_obj, expected_name) in stats_errors.iter().zip(expected_stats_errors) {
        let file_path = err_obj["file"].as_str().expect("file string");
        assert!(
            file_path.ends_with(expected_name),
            "Expected {expected_name}, got {file_path}"
        );
    }
    assert_eq!(stats_json["check"]["failed"], failed_files);
    assert_eq!(stats_json["check"]["warnings"], 3);

    // 4. Test `format -i` on a copy
    {
        let copy_root = std::env::temp_dir().join(format!(
            "tomet_parallel_test_copy_{}_{}",
            std::process::id(),
            nanoid::nanoid!()
        ));
        let mut copy_guard = TempVaultGuard::new(copy_root);
        copy_vault(vault_dir, &mut copy_guard, has_unreadable);
        let copy_str = copy_guard.path.to_str().unwrap();

        let fmt_i_out = run_cmd(&bin, &["format", "-i", copy_str]);
        assert_eq!(fmt_i_out.status.code(), Some(0));

        let stdout_text = String::from_utf8_lossy(&fmt_i_out.stdout);
        let stdout_lines: Vec<&str> = stdout_text.lines().collect();

        // Check stdout lines in order: 05-unformatted-a, b, c
        let expected_written = [
            copy_guard
                .path
                .join("05-unformatted-a.tmt")
                .display()
                .to_string(),
            copy_guard
                .path
                .join("05-unformatted-b.tmt")
                .display()
                .to_string(),
            copy_guard
                .path
                .join("05-unformatted-c.tmt")
                .display()
                .to_string(),
        ];
        assert_eq!(
            &stdout_lines[..3],
            &expected_written,
            "stdout lines should match formatted file paths in exact order"
        );
        assert_eq!(
            stdout_lines[3],
            format!("Formatted 3 of {total_files} file(s).")
        );

        // Check stderr lines for unreadable errors in order
        if has_unreadable {
            let stderr_text = String::from_utf8_lossy(&fmt_i_out.stderr);
            let stderr_lines: Vec<&str> = stderr_text.lines().collect();
            assert_eq!(
                stderr_lines.len(),
                3,
                "Expected 3 unreadable error lines on stderr; got:\n{stderr_text}"
            );
            for (line, suffix) in stderr_lines.iter().zip(["a", "b", "c"]) {
                let unreadable_name = format!("06-unreadable-{suffix}.tmt");
                assert!(
                    line.starts_with("Error reading ") && line.contains(&unreadable_name),
                    "Expected `Error reading .../{unreadable_name}` in line `{line}`"
                );
            }
        }

        // Verify resulting files
        for suffix in ["a", "b", "c"] {
            let unformatted_file = copy_guard.path.join(format!("05-unformatted-{suffix}.tmt"));
            let content = fs::read_to_string(&unformatted_file).unwrap();
            let expected_content = format!("a_{suffix}\n\nb_{suffix}\n");
            assert_eq!(
                content, expected_content,
                "File {unformatted_file:?} was not formatted correctly"
            );

            // Clean files remain unchanged
            let clean_file = copy_guard.path.join(format!("01-clean-{suffix}.tmt"));
            let clean_content = fs::read_to_string(&clean_file).unwrap();
            let expected_clean =
                format!("# Clean Document {suffix}\n\nThis is a clean document with no errors.\n");
            assert_eq!(clean_content, expected_clean);
        }
    }

    // 5. 50-run determinism checks
    assert_deterministic_50_runs(&["check"], vault_dir);
    assert_deterministic_50_runs(&["check", "--json"], vault_dir);
    assert_deterministic_50_runs(&["check", "-q"], vault_dir);
    assert_deterministic_50_runs(&["format", "--check"], vault_dir);
    assert_deterministic_50_runs(&["stats"], vault_dir);
    assert_deterministic_50_runs(&["stats", "--json"], vault_dir);
}
