use std::fs;
use std::path::PathBuf;

use crate::util::{format_parse_error, read, vault_for};

pub(crate) fn to_typst(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let vault = vault_for(file);
    let (mut doc, _bindings) = vault
        .parse(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    vault
        .prepare(&mut doc, file)
        .map_err(|e| anyhow::anyhow!("{}: index query failed -- {e}", file.display()))?;
    let typst = tomet_typst::to_typst(&doc);
    match out {
        Some(path) => fs::write(path, typst)?,
        None => println!("{typst}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_typst_writes_output_file() {
        let temp_dir = std::env::temp_dir().join("tomet_test_to_typst");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.tmt");
        let out_file = temp_dir.join("test_out.typ");

        fs::write(&src_file, "#[ Hello Typst ]\n").unwrap();

        let res = to_typst(&src_file, &Some(out_file.clone()));
        assert!(res.is_ok());
        let typst_text = fs::read_to_string(&out_file).unwrap();
        assert_eq!(typst_text, "= Hello Typst\n\n");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
