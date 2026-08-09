fn main() {
    let src_dir = std::path::Path::new("src");
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("parser.c").display()
    );
    cc::Build::new()
        .include(src_dir)
        .file(src_dir.join("parser.c"))
        .compile("tree-sitter-typedmark");
}
