use std::path::PathBuf;

use crate::util::{format_parse_error, read};

pub(crate) fn ast(file: &PathBuf, data: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    if data {
        let value = tomet_parser::parse_value(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{value:#?}");
    } else {
        let doc = tomet_parser::parse_document(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{doc:#?}");
    }
    Ok(())
}
