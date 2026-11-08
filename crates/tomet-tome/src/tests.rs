use std::ops::ControlFlow;

use tomet_ast::{Block, File, Inline};
use tomet_sort::{CollationSpec, Collator};
use tomet_tree::Visit;

use crate::{BuildError, Child, Folder, Heading, NamedFile, Node, Tome, children, walk};

fn parse(source: &str) -> File {
    tomet_ast_parser::parse_file(source).expect("test source parses")
}

fn build(files: &[(&str, &str)]) -> Tome {
    build_with(&Collator::codepoint(), files)
}

fn build_with(collator: &Collator, files: &[(&str, &str)]) -> Tome {
    Tome::build(
        "self.tmt",
        collator,
        None,
        files.iter().map(|(path, source)| (*path, parse(source))),
    )
    .expect("assembles")
}

fn text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Text(t) => Some(t.value.as_str()),
            _ => None,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// One line per node in walk order, indented by depth: `vault`, `dir NAME`, `file NAME`,
/// `section TITLE`.
fn outline(tome: &Tome) -> Vec<String> {
    fn go(node: Node<'_>, depth: usize, out: &mut Vec<String>) {
        let label = match node {
            Node::Vault(_) => "vault".to_string(),
            Node::Folder(f) => format!("dir {}", f.name),
            Node::File(d) => format!("file {}", d.name),
            Node::Section(s) => format!("section {}", text(&s.title)),
        };
        out.push(format!("{}{label}", "  ".repeat(depth)));
        children(node, &mut |child| go(child, depth + 1, out));
    }
    let mut out = Vec::new();
    go(tome.node(), 0, &mut out);
    out
}

fn folder<'a>(tome: &'a Tome, path: &[&str]) -> &'a Folder {
    let mut here = &tome.vault.root;
    for name in path {
        here = here
            .children
            .iter()
            .find_map(|c| match c {
                Child::Folder(f) if f.name == *name => Some(f),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no folder {name}"));
    }
    here
}

fn names(folder: &Folder) -> Vec<&str> {
    folder.children.iter().map(Child::name).collect()
}

fn declared(heading: Heading<'_>) -> Vec<&str> {
    heading
        .declarations
        .iter()
        .map(|d| d.name.name.as_str())
        .collect()
}

#[test]
fn the_entry_is_the_folders_own_node_and_not_a_child() {
    let tome = build(&[
        ("a.tmt", "=[ A ]\n"),
        ("self.tmt", "::kind(config)\n\n=[ Root ]\n"),
        ("sub/self.tmt", "::kind(note)\n"),
        ("sub/b.tmt", "=[ B ]\n"),
    ]);
    let root = &tome.vault.root;
    assert_eq!(
        root.entry.as_ref().map(|e| e.name.as_str()),
        Some("self.tmt")
    );
    assert_eq!(names(root), ["a.tmt", "sub"]);
    let sub = folder(&tome, &["sub"]);
    assert_eq!(sub.name, "sub");
    assert!(sub.entry.is_some());
    assert_eq!(names(sub), ["b.tmt"]);
}

#[test]
fn a_folder_without_an_entry_has_an_empty_heading_part() {
    let tome = build(&[("notes/a.tmt", "::kind(note)\n")]);
    let notes = folder(&tome, &["notes"]);
    assert!(notes.entry.is_none());
    let heading = Node::Folder(notes).heading();
    assert!(heading.declarations.is_empty());
    assert!(heading.title.is_empty());
    assert!(heading.connects.is_empty());
    assert!(heading.id.is_none());
    assert!(heading.data.args.is_none() && heading.data.value.is_none());
}

#[test]
fn a_custom_entry_name_is_the_one_that_counts() {
    let tome = Tome::build(
        "index.tmt",
        &Collator::codepoint(),
        None,
        [
            ("d/index.tmt", parse("::kind(note)\n")),
            ("d/self.tmt", parse("")),
        ],
    )
    .unwrap();
    let d = folder(&tome, &["d"]);
    assert_eq!(d.entry.as_ref().unwrap().name, "index.tmt");
    assert_eq!(names(d), ["self.tmt"]);
}

#[test]
fn children_are_in_collation_order_whatever_order_they_arrive_in() {
    let files = [
        ("b.tmt", ""),
        ("Z/x.tmt", ""),
        ("a/x.tmt", ""),
        ("a.tmt", ""),
        ("B.tmt", ""),
    ];
    let reversed: Vec<_> = files.iter().rev().copied().collect();
    let forward = build(&files);
    assert_eq!(
        names(&forward.vault.root),
        ["B.tmt", "Z", "a", "a.tmt", "b.tmt"]
    );
    assert_eq!(forward, build(&reversed));
}

#[test]
fn the_collator_decides_and_ties_fall_to_code_points() {
    // `a` and `B` share one weight, so only the secondary level and then the code points split
    // them; `z` is ordered before both.
    let spec = CollationSpec::new()
        .token("z")
        .group(["a", "A"])
        .group(["b", "B"]);
    let collator = spec.compile().unwrap();
    let files = [("b", ""), ("A", ""), ("z", ""), ("a", ""), ("B", "")];
    let tome = build_with(&collator, &files);
    assert_eq!(names(&tome.vault.root), ["z", "a", "A", "b", "B"]);
}

#[test]
fn nested_folders_nest() {
    let tome = build(&[("a/b/c/d.tmt", "=[ D ]\n")]);
    assert_eq!(
        outline(&tome),
        [
            "vault",
            "  dir ",
            "    dir a",
            "      dir b",
            "        dir c",
            "          file d.tmt",
            "            section D",
        ]
    );
}

#[test]
fn sections_are_borrowed_from_their_documents_and_nest() {
    let tome = build(&[("a.tmt", "=[ One ]\n\n==[ Two ]\n\ntext\n\n=[ Three ]\n")]);
    let Child::File(doc) = &tome.vault.root.children[0] else {
        panic!("a document")
    };
    let Block::Section(one) = &doc.file.body[0] else {
        panic!("a section")
    };
    let mut top = Vec::new();
    children(Node::File(doc), &mut |n| top.push(n));
    assert_eq!(top.len(), 2);
    let Node::Section(first) = top[0] else {
        panic!("a section node")
    };
    assert!(std::ptr::eq(first, one), "a section is not copied");
    let mut inner = Vec::new();
    children(top[0], &mut |n| inner.push(n));
    assert!(matches!(inner.as_slice(), [Node::Section(s)] if text(&s.title) == "Two"));
}

#[test]
fn an_entry_documents_sections_come_before_its_folders_children() {
    let tome = build(&[("self.tmt", "=[ Intro ]\n"), ("a.tmt", "=[ A ]\n")]);
    assert_eq!(
        outline(&tome),
        [
            "vault",
            "  dir ",
            "    section Intro",
            "    file a.tmt",
            "      section A",
        ]
    );
}

#[test]
fn the_config_document_heads_the_vault() {
    let config = NamedFile {
        name: "vault.config.tmt".into(),
        file: parse("::kind(config)\n\n=[ Vault ]\n"),
    };
    let tome = Tome::build::<&str>("self.tmt", &Collator::codepoint(), Some(config), []).unwrap();
    assert_eq!(declared(tome.node().heading()), ["kind"]);
    assert_eq!(outline(&tome), ["vault", "  section Vault", "  dir "]);
}

#[test]
fn every_level_has_its_heading_part() {
    let tome = build(&[
        ("self.tmt", "::kind(root)\n::meta(title: R)\n"),
        ("d/self.tmt", "::kind(dir)\n"),
        ("d/f.tmt", "::kind(file)\n\n=[ Sec ]#(s1):meta(k: v)\n"),
    ]);
    assert_eq!(declared(tome.node().heading()), Vec::<&str>::new());
    assert_eq!(
        declared(Node::Folder(&tome.vault.root).heading()),
        ["kind", "meta"]
    );
    let d = folder(&tome, &["d"]);
    assert_eq!(declared(Node::Folder(d).heading()), ["kind"]);
    let Child::File(file) = &d.children[0] else {
        panic!("a document")
    };
    assert_eq!(declared(Node::File(file).heading()), ["kind"]);

    let mut sections = Vec::new();
    children(Node::File(file), &mut |n| sections.push(n));
    let heading = sections[0].heading();
    assert!(heading.declarations.is_empty());
    assert_eq!(text(heading.title), "Sec");
    assert_eq!(heading.id.map(|id| id.0.as_str()), Some("s1"));
    assert_eq!(heading.connects.len(), 1);
}

#[test]
fn walk_can_skip_and_stop() {
    let tome = build(&[("a/x.tmt", "=[ X ]\n"), ("b/y.tmt", "=[ Y ]\n")]);

    let mut seen = Vec::new();
    let flow = walk(tome.node(), &mut |n| {
        seen.push(matches!(n, Node::Section(_)));
        match n {
            Node::Folder(f) if f.name == "a" => Visit::<()>::Skip,
            _ => Visit::Descend,
        }
    });
    assert_eq!(flow, ControlFlow::Continue(()));
    // vault, root, a (skipped), b, y.tmt, Y
    assert_eq!(seen.len(), 6);
    assert_eq!(seen.iter().filter(|s| **s).count(), 1);

    let mut count = 0;
    let flow = walk(tome.node(), &mut |n| {
        count += 1;
        match n {
            Node::File(d) => Visit::Break(d.name.clone()),
            _ => Visit::Descend,
        }
    });
    assert_eq!(flow, ControlFlow::Break("x.tmt".to_string()));
    assert_eq!(
        count, 4,
        "vault, root, a, x.tmt: nothing is shown after a break"
    );
}

#[test]
fn documents_that_cannot_make_a_tree_are_refused() {
    let build = |files: &[&str]| {
        Tome::build(
            "self.tmt",
            &Collator::codepoint(),
            None,
            files.iter().map(|p| (*p, parse(""))),
        )
        .map(|_| ())
    };
    assert_eq!(
        build(&["a.tmt", "a.tmt"]),
        Err(BuildError::Duplicate("a.tmt".into()))
    );
    assert_eq!(
        build(&["d/self.tmt", "d/self.tmt"]),
        Err(BuildError::Duplicate("d/self.tmt".into()))
    );
    assert_eq!(
        build(&["a", "a/b.tmt"]),
        Err(BuildError::FileAndFolder("a/b.tmt".into()))
    );
    assert_eq!(
        build(&["a/b.tmt", "a"]),
        Err(BuildError::FileAndFolder("a".into()))
    );
    for bad in ["", "/a.tmt", "a//b.tmt", "./a.tmt", "a/../b.tmt", "a/"] {
        assert_eq!(
            build(&[bad]),
            Err(BuildError::InvalidPath(bad.into())),
            "{bad:?}"
        );
    }
}
