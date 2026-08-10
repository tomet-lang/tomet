fn main() {
    let src_dir = std::path::Path::new("src");
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("parser.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("scanner.c").display()
    );
    cc::Build::new()
        .include(src_dir)
        .file(src_dir.join("parser.c"))
        .file(src_dir.join("scanner.c"))
        .compile("tree-sitter-typedmark");
}
