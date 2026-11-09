use super::*;

/// A directory in a fresh temporary place, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tomet-standalone-{nanos}-{name}"));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A vault that defines the macro `gh`.
fn vault(dir: &Dir) {
    dir.write(
        "vault.config.tmt",
        "::kind(config)\n::macro(\n  gh: \"https://x/issues/${1}\"\n)\n",
    );
}

#[test]
fn a_file_in_a_vault_is_written_out_standalone() {
    let dir = Dir::new("ok");
    vault(&dir);
    let file = dir.write("docs/a.tmt", "See ${macro.gh(1)} in ${self.path}.\n");
    let out = dir.path("out");
    fs::create_dir_all(&out).unwrap();
    run(&file, Some(&out), false).expect("nothing is left");

    let text = fs::read_to_string(out.join("a.tmt")).unwrap();
    assert!(text.contains("::profile(standalone)"), "{text}");
    assert!(text.contains("::macro"), "{text}");
    assert!(text.contains("${macro.gh(1)}"), "{text}");
    assert!(text.contains("docs/a.tmt"), "{text}");
    assert!(!text.contains("${self.path}"), "{text}");

    // The promise is kept: the validator finds nothing in what was written.
    let written = tomet_ast_parser::parse_file(&text).unwrap();
    assert_eq!(tomet_validator::standalone_findings(&written), vec![]);
    // The original is as it was.
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "See ${macro.gh(1)} in ${self.path}.\n"
    );
}

#[test]
fn it_will_not_write_over_the_file_it_reads() {
    let dir = Dir::new("same");
    vault(&dir);
    let source = "In ${self.path}.\n";
    let file = dir.write("a.tmt", source);

    let error = run(&file, Some(&file), false).unwrap_err().to_string();
    assert!(error.contains("write somewhere else"), "{error}");
    // Nor by naming the directory it sits in.
    let error = run(&file, Some(&dir.0), false).unwrap_err().to_string();
    assert!(error.contains("write somewhere else"), "{error}");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn only_a_file_can_be_taken_out_of_its_vault() {
    let dir = Dir::new("notfile");
    assert!(run(&dir.0, None, false).is_err());
    assert!(run(Path::new("-"), None, false).is_err());
}

#[test]
fn something_left_is_an_error_but_the_file_is_still_written() {
    let dir = Dir::new("left");
    vault(&dir);
    let file = dir.write("a.tmt", "@frobnicate[ x ] in ${self.path}\n");
    let out = dir.path("a.out.tmt");

    let error = run(&file, Some(&out), false).unwrap_err().to_string();
    assert!(error.contains("not standalone yet"), "{error}");
    let text = fs::read_to_string(&out).expect("written all the same");
    assert!(text.contains("frobnicate"), "{text}");
    assert!(text.contains("a.tmt"), "{text}");
}

#[test]
fn a_meta_field_is_frozen_only_when_asked() {
    let dir = Dir::new("fields");
    vault(&dir);
    let file = dir.write("a.tmt", "::meta(title: Hello)\n\n${self.meta.title}\n");

    let kept = dir.path("kept.tmt");
    assert!(run(&file, Some(&kept), false).is_err());
    assert!(
        fs::read_to_string(&kept)
            .unwrap()
            .contains("${self.meta.title}")
    );

    let frozen = dir.path("frozen.tmt");
    run(&file, Some(&frozen), true).expect("nothing is left");
    assert!(!fs::read_to_string(&frozen).unwrap().contains("${self.meta"));
}
