# tove (Tomet Object & Value Expression)

TOVE is a lightweight, expressive data language and parser crate for Tomet.

It provides:
- **Pure data syntax parsing**: parses scalars, strings, maps (`key: value` and `{ ... }`), lists (`list(...)`), and expressions into `tomet_ast::Value`.
- **Serde integration**: serialize and deserialize Rust structs via `tove::from_str` and `tove::to_string`, comparable to `toml` or `ron`.
- **Extensibility**: hooks for embeddable values in host document languages like Tomet (`tomet-syntax-parser`).

## Usage

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Config {
    title: String,
    port: u16,
    enabled: bool,
    tags: Vec<String>,
}

fn main() {
    let text = r#"
        title: "My Service"
        port: 8080
        enabled: true
        tags: list("web", "api")
    "#;

    let config: Config = tove::from_str(text).unwrap();
    println!("{config:?}");

    let rendered = tove::to_string(&config).unwrap();
    println!("{rendered}");
}
```
