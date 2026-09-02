//! Generates `tests/SYNTAX.md`: every construct the parser currently
//! accepts, with a minimal example and the AST it produces.
//!
//! **Why this exists.** `docs/spec/` is normative -- it says what the
//! language *should* accept -- and it is hand-written, so nothing forces
//! it to match the parser. It has drifted before and will again. This
//! file is the descriptive counterpart: it says what the implementation
//! *does* accept, and it cannot lie, because every line of it is produced
//! by running `tomet-parser` over the table below.
//!
//! Reading the two side by side is the point. Where they disagree, the
//! disagreement is the finding, not a bug in this file.
//!
//! The report is compared against the committed `tests/SYNTAX.md` the
//! same way `tests/ref/` snapshots are, so a grammar change that is not
//! reflected here fails the suite:
//!
//! ```bash
//! cargo test -p tomet-tests --test syntax_report
//! TOMET_UPDATE_REF=1 cargo test -p tomet-tests --test syntax_report
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, InterpExpr, Sigil, Value};
use tomet_parser::parse_document;

/// One documented construct: a heading it files under, a one-line note in
/// Japanese, and the source to run through the parser.
struct Case {
    /// `None` continues the previous section.
    section: Option<&'static str>,
    note: &'static str,
    src: &'static str,
}

const fn case(section: Option<&'static str>, note: &'static str, src: &'static str) -> Case {
    Case {
        section,
        note,
        src,
    }
}

/// Every construct, in the order the report presents them.
const CASES: &[Case] = &[
    // ---- headings ---------------------------------------------------
    case(Some("見出し"), "`#[ ... ]` は `#heading` の名前省略形", "#[ タイトル ]\n"),
    case(None, "`#` の数がレベル", "##[ 節 ]\n"),
    case(None, "末尾の `{...}` は属性", "#[ タイトル ]{ id: intro }\n"),
    case(None, "`:` を挟んでも同じ", "#[ タイトル ]:{ id: intro }\n"),
    // ---- block elements ---------------------------------------------
    case(
        Some("ブロック要素 `#name`"),
        "名前だけ。グループがなくても行末で要素になる",
        "#memo\n",
    ),
    case(None, "`(args)` `[content]` `{value}` は各1個まで、順不同", "#memo(a: 1)[ 本文 ]{ b: 2 }\n"),
    case(None, "順序を入れ替えても同じ木になる", "#memo[ 本文 ](a: 1)\n"),
    case(None, "`.` 区切りの名前空間", "#deck.bookmark(name: foo)[ x ]\n"),
    case(None, "名前空間は多段でもよい", "#a.b.c(x: 1)\n"),
    // ---- inline elements --------------------------------------------
    case(
        Some("インライン要素 `@name`"),
        "行の途中で要素になる",
        "文中の @link(target: \"https://example.com\")[リンク] です。\n",
    ),
    case(None, "名前空間つき", "文中の @deck.badge(2)[印] です。\n"),
    case(None, "名前なしの `@`", "@(url: \"https://example.com\")[リンク]\n"),
    // ---- fall back to text ------------------------------------------
    case(
        Some("文字列に落ちる場合"),
        "`#` と名前の間に空白があると要素にならない",
        "# 見出しではない\n",
    ),
    case(None, "`#` が2つ以上のときは `[` が必要", "##memo(a: 1)\n"),
    case(None, "ASCII でない名前は要素にならない", "#タグ\n"),
    case(None, "`@` の後にグループが続かなければ文字列", "連絡は me@example.com まで\n"),
    case(None, "行中の `#` は普通の文字", "C# と F# の話\n"),
    case(None, "`<` はもうシジルではない", "型は Vec<T> と書く\n"),
    // ---- the +++ fence ----------------------------------------------
    case(
        Some("`+++` フェンス"),
        "閉じる `+++` だけの行まで逐語。括弧も引用符もそのまま",
        "#memo+++\ndon't forget [this]\n+++\n",
    ),
    case(
        None,
        "本文に `+++` があるときは長い走りで囲む",
        "#memo++++\n+++\nまだ本文\n++++\n",
    ),
    case(None, "閉じないまま EOF に達したらそこで終わる", "#memo+++\n閉じない\n"),
    case(
        None,
        "`${...}` は展開されず逐語で残る",
        "#config(format:json)+++\n{\"gh\": \"x/${1}\"}\n+++\n",
    ),
    case(
        None,
        "引用符の中の `}` で本文が途切れない",
        "#meta(format:yaml)+++\na: \"}\"\nb: 1\n+++\n",
    ),
    case(
        None,
        "`format:` は解釈だけを決め、字句解析には影響しない",
        "#zzz(format:yaml)+++\na: 1\n+++\n",
    ),
    // ---- value groups -----------------------------------------------
    case(Some("`{...}` グループ"), "`key: value` の並び", "#memo{ a: 1, b: two }\n"),
    case(None, "要素の並び", "#links{\n  (1)[ ひとつ ]\n  (2)[ ふたつ ]\n}\n"),
    case(None, "対と要素の混在。並び順は保たれる", "#deck.card{ t: x, (a)[ y ], u: z }\n"),
    case(None, "空のグループ", "#memo{}\n"),
    // ---- args and the value DSL -------------------------------------
    case(Some("`(args)` と値の文法"), "`key: value`", "#memo(a: 1, b: two)\n"),
    case(None, "位置引数（キーなし）", "#codeblock(rust)\n"),
    case(None, "列", "#memo(xs: [1, 2, 3])\n"),
    case(None, "入れ子のマップ", "#memo(m: { x: 1 })\n"),
    case(None, "引用符つき文字列", "#memo(s: \"a, b: c\")\n"),
    case(None, "スカラーは型が推論される", "#memo(i: 1, f: 1.5, b: true, n: null)\n"),
    // ---- lists ------------------------------------------------------
    case(Some("リスト"), "`-` が非順序、`-.` が順序", "- ひとつ\n- ふたつ\n"),
    case(None, "順序つき", "-. ひとつ\n-. ふたつ\n"),
    case(None, "ネスト", "- 親\n-- 子\n"),
    case(None, "`(marker)` と末尾の `{attrs}`", "- (12:01) 本文 {id: a}\n"),
    // ---- inline markup ----------------------------------------------
    case(Some("インライン記法"), "強調・太字・マーク・コード", "*em* と **strong** と ==mark== と `code`\n"),
    case(None, "自動リンク", "見て https://example.com/x ください\n"),
    // ---- breaks and code --------------------------------------------
    case(Some("区切りとコードブロック"), "区切り線", "---\n"),
    case(None, "見出しつき区切り線", "---[ 章題 ]---\n"),
    case(None, "``` フェンス。中身は逐語", "```rust\nlet y = @T; *ptr\n```\n"),
    // ---- interpolation ----------------------------------------------
    case(Some("補間 `${...}`"), "識別子", "${name}\n"),
    case(None, "メンバ参照", "${a.b.c}\n"),
    case(None, "関数呼び出し", "${sum(1, 2)}\n"),
    case(None, "`$name(...)` 形式", "$uuid()\n"),
    // ---- connect ----------------------------------------------------
    case(Some("コネクト `:`"), "`:{...}` は値をマージ", "#task[ A ]:{ id: t1 }\n"),
    case(None, "`:(...)` は args をマージ", "#task(a: 1):(b: 2)\n"),
    case(None, "id 指定のリモート接続", "#id(taskA):{ priority: high }\n"),
    case(None, "複数 id", "#id([taskA, taskB]):{ tag: house }\n"),
    // ---- comments ---------------------------------------------------
    case(Some("コメント"), "行コメント", "// 消える\n本文\n"),
    case(None, "ブロックコメント", "本文 /* 消える */ の続き\n"),
];

/// Constructs that were retired, are easy to misread, or are rejected.
///
/// Not all of these fail to parse -- and that is deliberate. A construct
/// that was *supposed* to be removed but still parses is exactly the kind
/// of gap this report exists to surface, so it is listed with whatever it
/// actually does rather than with what it was meant to do.
const REJECTED: &[Case] = &[
    case(
        Some("撤去された構文"),
        "`<T>` シジルは廃止。ただの文字列になる",
        "<memo>[ x ]\n",
    ),
    case(
        Some("紛らわしいが、これが正しい"),
        "`(content:raw)` は普通の引数。`[...]` の解釈を変えない",
        "#memo(content:raw)[\n1行目\n2行目\n]\n",
    ),
    case(
        Some("未完了: 撤去する予定だがまだ通る"),
        "`@[ ... ]` は名前も args もない `@`。`classify` は \
         `Custom(\"at\")` という意味のない種別を返す。\
         `.agents/tasks/sigil-shape-axis-and-namespaces.md` は撤去すると \
         書いているが、実装はまだ受け付けている",
        "@[ x ]\n",
    ),
    case(
        Some("受け付けない書き方"),
        "`{...}` に列は書けない。`+++` フェンスを使う",
        "#memo{[1, 2, 3]}\n",
    ),
    case(None, "`{...}` に裸のスカラーも書けない", "#memo{hello}\n"),
    case(None, "閉じない `[`", "#memo[ 閉じない\n"),
    case(
        Some("仕様にあるが未実装"),
        "入れ子の呼び出し（`docs/spec/builtin-functions.tmt`）",
        "${ref(id(asdf).contents(default))}\n",
    ),
    case(
        None,
        "正規表現リテラル（`docs/design/ideas/idea.tmt`）",
        "$regex(/*.svg/g)\n",
    ),
];

#[test]
fn syntax_report_matches_the_committed_file() {
    let report = render_report();
    let path = repo_root().join("tests/SYNTAX.md");

    if std::env::var_os("TOMET_UPDATE_REF").is_some_and(|v| !v.is_empty() && v != "0") {
        std::fs::write(&path, &report).expect("write SYNTAX.md");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed != report {
        let stored = repo_root().join("tests/store/SYNTAX.md");
        if let Some(parent) = stored.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&stored, &report);
        panic!(
            "tests/SYNTAX.md is out of date with the parser.\n\
             That is the point of this file: the grammar changed and the\n\
             inventory did not. Read the diff, then accept it with:\n\
             \x20   TOMET_UPDATE_REF=1 cargo test -p tomet-tests --test syntax_report\n\
             actual written to {stored:?}"
        );
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests/ has a parent")
        .to_path_buf()
}

fn render_report() -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    out.push_str("\n## 目次\n\n");
    for case in CASES.iter().chain(REJECTED) {
        if let Some(section) = case.section {
            let _ = writeln!(out, "- [{section}](#{})", anchor(section));
        }
    }

    for case in CASES {
        emit_case(&mut out, case, false);
    }
    for case in REJECTED {
        emit_case(&mut out, case, true);
    }

    out.push_str(FOOTER);
    out
}

fn emit_case(out: &mut String, case: &Case, rejected_section: bool) {
    if let Some(section) = case.section {
        let _ = write!(out, "\n## {section}\n");
    }
    let _ = write!(out, "\n### {}\n\n```tmt\n{}```\n\n", case.note, case.src);

    match parse_document(case.src) {
        Ok(doc) => {
            let _ = write!(out, "```\n{}```\n", dump_document(&doc));
        }
        Err(e) => {
            let _ = write!(out, "```\nparse error: {e}\n```\n");
        }
    }
    // A case in the rejected half that parses anyway is worth calling
    // out: either it means something other than it looks, or it was meant
    // to be gone and is not.
    if rejected_section && parse_document(case.src).is_ok() {
        out.push_str("\n> **パースは通る。** 上の AST が実際の意味。\n");
    }
}

fn anchor(section: &str) -> String {
    section
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '`' | '(' | ')' | '{' | '}' | '[' | ']' | '.' | '+' | '$' | '#' | '@' | ':'
            )
        })
        .map(|c| if c == ' ' { '-' } else { c })
        .collect::<String>()
        .to_lowercase()
}

// ---------------------------------------------------------------------
// Compact AST dump
// ---------------------------------------------------------------------
//
// `{:?}` on a `Document` is unreadable here: every node carries a `Span`
// with three fields, which is most of the output and none of the meaning.
// This prints the shape only.

fn dump_document(doc: &Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        dump_block(&mut out, block, 0);
    }
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn dump_block(out: &mut String, block: &Block, depth: usize) {
    match block {
        Block::Paragraph(p) => {
            indent(out, depth);
            out.push_str("Paragraph\n");
            for inline in &p.content {
                dump_inline(out, inline, depth + 1);
            }
        }
        Block::Element(el) => dump_element(out, el, depth),
    }
}

fn dump_inline(out: &mut String, inline: &Inline, depth: usize) {
    match inline {
        Inline::Text(t) => {
            indent(out, depth);
            let _ = writeln!(out, "Text {:?}", t.value);
        }
        Inline::Element(el) => dump_element(out, el, depth),
    }
}

fn dump_element(out: &mut String, el: &Element, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "{}", sigil_str(&el.sigil));

    if let Some(args) = &el.args {
        indent(out, depth + 1);
        let _ = writeln!(out, "args    {}", value_str(args));
    }
    if let Some(content) = &el.content {
        indent(out, depth + 1);
        out.push_str("content\n");
        for inline in content {
            dump_inline(out, inline, depth + 2);
        }
    }
    match &el.value {
        Some(ElementValue::Raw(body)) => {
            indent(out, depth + 1);
            let _ = writeln!(out, "raw     {body:?}");
        }
        Some(ElementValue::Interp(expr)) => {
            indent(out, depth + 1);
            let _ = writeln!(out, "interp  {}", interp_str(expr));
        }
        Some(ElementValue::Group(entries)) => {
            indent(out, depth + 1);
            if entries.is_empty() {
                out.push_str("group   (空)\n");
            } else {
                out.push_str("group\n");
            }
            for entry in entries {
                match entry {
                    Entry::Pair(k, v) => {
                        indent(out, depth + 2);
                        let _ = writeln!(out, "{k}: {}", value_str(v));
                    }
                    Entry::Element(child) => dump_element(out, child, depth + 2),
                }
            }
        }
        None => {}
    }
    if let Some(children) = &el.children {
        indent(out, depth + 1);
        out.push_str("children\n");
        for child in children {
            dump_block(out, child, depth + 2);
        }
    }
}

fn sigil_str(sigil: &Sigil) -> String {
    match sigil {
        Sigil::Block(name) => format!("Block  #{name}"),
        Sigil::Inline(Some(name)) => format!("Inline @{name}"),
        Sigil::Inline(None) => "Inline @".to_string(),
        Sigil::Bare => "Bare".to_string(),
        Sigil::Dollar => "Interp $".to_string(),
    }
}

fn value_str(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("{s:?}"),
        Value::Seq(items) => {
            let inner: Vec<String> = items.iter().map(value_str).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(entries) => {
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    if k.is_empty() {
                        // The parser's positional-entry sentinel.
                        format!("(位置) {}", value_str(v))
                    } else {
                        format!("{k}: {}", value_str(v))
                    }
                })
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

fn interp_str(expr: &InterpExpr) -> String {
    use tomet_ast::{InterpExprKind as K, Literal};
    match &expr.kind {
        K::Identifier(name) => name.clone(),
        K::Literal(Literal::Int(i)) => i.to_string(),
        K::Literal(Literal::Float(f)) => f.to_string(),
        K::Literal(Literal::String(s)) => format!("{s:?}"),
        K::Call { callee, args } => {
            let inner: Vec<String> = args.iter().map(interp_str).collect();
            format!("{}({})", interp_str(callee), inner.join(", "))
        }
        K::Member { object, member } => format!("{}.{member}", interp_str(object)),
        K::NamedArg { name, value } => format!("{name}: {}", interp_str(value)),
    }
}

const HEADER: &str = r#"# 実装されている構文

**このファイルは生成物です。手で編集しないでください。**

出どころは `tests/src/syntax_report.rs` の表と `tomet-parser` そのもので、
`docs/spec/` ではありません。ここに載っているのは「言語がこう受け付ける
*べき*」ではなく「実装が今こう受け付ける」です。

```bash
cargo test -p tomet-tests --test syntax_report              # 検証
TOMET_UPDATE_REF=1 cargo test -p tomet-tests --test syntax_report  # 更新
```

文法を変えてこのファイルを更新し忘れると、テストが落ちます。

## `docs/spec/` との関係

`docs/spec/` は**規範的**（言語が何を受け付けるべきか）で、手書きです。
このファイルは**記述的**（実装が何を受け付けるか）で、生成物です。
両者が食い違っているとき、食い違いそのものが読み取るべき情報であって、
どちらかを黙って他方に合わせるべきではありません。

`AST` 欄は `Span` を省いた木の形です。`Span` は全ノードが持っていますが、
意味を持たないのでここには出しません。
"#;

const FOOTER: &str = r#"
---

この一覧に載っていない構文は、実装されていないか、この表に追加し忘れて
いるかのどちらかです。後者を見つけたら `tests/src/syntax_report.rs` の
`CASES` に足してください。
"#;
