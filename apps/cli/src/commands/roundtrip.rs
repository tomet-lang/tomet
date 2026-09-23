use std::path::PathBuf;

use crate::util::read;

pub(crate) fn roundtrip(file: &PathBuf) -> anyhow::Result<()> {
    let src = read(file)?;

    // `serde_json::Value` stands in for "some arbitrary Serialize/
    // Deserialize type" here, since it can hold any shape without a
    // fixed struct -- a real caller (e.g. tomet) would use its own
    // `#[derive(Serialize, Deserialize)]` struct instead.
    let value: serde_json::Value = tove::from_str(&src)?;
    println!("parsed:\n{}", serde_json::to_string_pretty(&value)?);

    let rendered = tove::to_string(&value)?;
    println!("\nrendered back:\n{rendered}");

    let reparsed: serde_json::Value = tove::from_str(&rendered)?;
    if reparsed == value {
        println!("\nround-trip OK");
        Ok(())
    } else {
        println!("\nround-trip MISMATCH");
        Err(anyhow::anyhow!(
            "re-parsing the rendered output produced a different value"
        ))
    }
}
