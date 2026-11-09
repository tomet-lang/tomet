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

use tomet_ast::{
    Bare, Block, Connect, Data, Declaration, ElementView, Entry, File, Group, Hole, Inline,
    InterpExpr, Interpolation, List, ListItem, Placement, Value,
};
use tomet_ast_parser::parse_file;

/// One documented construct: a heading it files under, a one-line note in
/// Japanese, and the source to run through the parser.
struct Case {
    /// `None` continues the previous section.
    section: Option<&'static str>,
    note: &'static str,
    src: &'static str,
}

const fn case(section: Option<&'static str>, note: &'static str, src: &'static str) -> Case {
    Case { section, note, src }
}

/// Every construct, in the order the report presents them.
const CASES: &[Case] = &[
    // ---- headings ---------------------------------------------------
    case(
        Some("見出し"),
        "見出しは `=`/`==` が担う。`#` は見出しのマーカーではなく、どんな\
         数重ねても、空白や括弧を続けても、ただの文字として地の文に落ちる",
        "#[ タイトル ]\n",
    ),
    case(None, "重ねても同じ", "##[ 節 ]\n"),
    // ---- block elements ---------------------------------------------
    case(
        Some("ブロック配置の要素"),
        "行に要素しかなければブロック。中身が空でも `()` か `[]` を書く。\
         グループの無い `@memo` だけの行は要素ではなくエラー（下の「主グループの無いネクサス」）",
        "@memo()\n",
    ),
    case(
        None,
        "`(args)` `[content]` `{value}` は各1個まで、順不同。\
         ただし `(args)` か `[content]` のどちらかは要る（下の「ネクサスと主グループ」）",
        "@memo(a: 1)[ 本文 ]{ b: 2 }\n",
    ),
    case(
        None,
        "順序を入れ替えても同じ木になる",
        "@memo[ 本文 ](a: 1)\n",
    ),
    case(
        None,
        "`.` 区切りの名前空間",
        "@deck.bookmark(name: foo)[ x ]\n",
    ),
    case(None, "名前空間は多段でもよい", "@a.b.c(x: 1)\n"),
    case(
        None,
        "行頭にあっても、後ろに続きがあればブロックにならない。\
         段落の書き出しとして読む",
        "@link(target: \"https://example.com\")[Tomet] は軽量マークアップ言語です。\n",
    ),
    case(
        None,
        "折り返した行の先頭にある要素も地の文の一部。\
         段落を切るのは空行",
        "この言語の名前は\n@link(target: \"https://example.com\")[Tomet] といいます。\n",
    ),
    // ---- inline elements --------------------------------------------
    case(
        Some("インライン配置の要素"),
        "行の途中の要素は地の文の一部",
        "文中の @link(target: \"https://example.com\")[リンク] です。\n",
    ),
    case(None, "名前空間つき", "文中の @deck.badge(2)[印] です。\n"),
    // ---- fall back to text ------------------------------------------
    case(
        Some("文字列に落ちる場合"),
        "`#` はどんな綴りでも地の文",
        "#タグ ではない\n",
    ),
    case(None, "ASCII でない名前は要素にならない", "@タグ\n"),
    case(
        None,
        "`@` の後にグループが続かなければ文字列",
        "連絡は me@example.com まで\n",
    ),
    case(
        None,
        "行の途中でも、グループの無い `@名前` は地の文。メンションや宛先が\
         エラーにならないための線引き",
        "ping @someone about it\n",
    ),
    case(None, "行中の `#` は普通の文字", "C# と F# の話\n"),
    case(None, "`<` はもうシジルではない", "型は Vec<T> と書く\n"),
    // ---- fenced raw body nested in [content] -------------------------
    case(
        Some("`[content]` に入れ子になった backtick フェンス"),
        "`[content]` は要素本体と同じ再帰文法を共有するので、backtick\
         フェンスも入れ子にできる。閉じる backtick の行まで逐語、\
         括弧も引用符もそのまま",
        "@memo[\n```\ndon't forget [this]\n```\n]\n",
    ),
    case(
        None,
        "本文に backtick の行があるときは長い走りで囲む",
        "@memo[\n````\n```\nまだ本文\n````\n]\n",
    ),
    case(
        None,
        "`${...}` は展開されず逐語で残る",
        "::config[\n```\n{\"gh\": \"x/${1}\"}\n```\n]\n",
    ),
    // ---- the nexus rule ---------------------------------------------
    case(
        Some("ネクサスと主グループ"),
        "`@名前` `::名前` はネクサスで、主グループを要る。主グループは \
         `(args)` と `[content]`。`{value}` `#(id)` `::コネクト` は付加のグループで、\
         単独では置けない。空でも主グループは書く",
        "@memo()\n",
    ),
    case(None, "`[content]` だけでもよい", "@memo[ 本文 ]\n"),
    case(
        None,
        "付加のグループは主グループのあとに付く",
        "@memo(){ a: 1 }#(first)\n",
    ),
    case(
        None,
        "主グループは付加のグループのあとでもよい。要るのは、どこかに一つあること",
        "@memo{ a: 1 }(b: 2)\n",
    ),
    case(
        None,
        "`|` の連なりも主グループ。`@table` だけの行でも、次の行の `|` が主グループになる。\
         `()` `[]` の無い、唯一の要素の書き方",
        "@table\n|[ a ][ b ]\n|[ 1 ][ 2 ]\n",
    ),
    case(
        None,
        "リスト項目に付けるときも同じ。`-@名前` は主グループを要る",
        "-@memo(a: 1)\n-@memo[ 本文 ]\n",
    ),
    case(
        None,
        "名前のないコネクト `::{...}` `::(...)` は、付いた先（項目・見出し・要素）の\
         データに入る。`{...}` が単独で置ける唯一の場所",
        "- 項目 ::{ k: v }\n= 見出し ::(k: v)\n",
    ),
    case(
        None,
        "名前つきコネクトも主グループを要る。`::rule(...)` は書けて、`::rule{...}` は書けない（下の「主グループの無いネクサス」）",
        "- 項目 ::rule(allow: (card))\n",
    ),
    case(None, "水平線は `@hr()`。`@hr` だけの行はエラー", "@hr()\n"),
    // ---- value groups -----------------------------------------------
    case(
        Some("`{...}` グループ"),
        "`{...}` は付加のグループ。`(...)` か `[...]` を伴う要素に付き、\
         単独では要素にならない（`@memo{ ... }` は下の「主グループの無いネクサス」）。\
         中身は `key: value` の並び",
        "@memo(){ a: 1, b: two }\n",
    ),
    case(
        None,
        "要素の並び",
        "@memo(){\n  (1)[ ひとつ ]\n  (2)[ ふたつ ]\n}\n",
    ),
    case(
        None,
        "対と要素の混在。並び順は保たれる",
        "@deck.card(){ t: x, (a)[ y ], u: z }\n",
    ),
    case(
        None,
        "裸のエントリも名前つき要素と同じ三つのグループを取る。\
         シジルが無いだけで、読む道は同じ",
        "@memo(){ (1)[ ひとつ ]{ id: a } }\n",
    ),
    case(
        None,
        "グループを次の行に置いてもよい。`@name` と同じ",
        "@memo(){\n  (1)\n  [ ひとつ ]\n}\n",
    ),
    case(None, "空のグループ", "@memo(){}\n"),
    case(
        None,
        "引用符つきのキー。パスや空白を含むキーはこう書く。引用符つきの文字列は、直後に `:` があるときだけキーになる",
        "@memo(){ \"a/b.tmt\": 1, plain: 2 }\n",
    ),
    // ---- args and the value DSL -------------------------------------
    case(
        Some("`(args)` と値の文法"),
        "`key: value`",
        "@memo(a: 1, b: two)\n",
    ),
    case(None, "位置引数（キーなし）", "@raw(rust)\n"),
    case(
        None,
        "列。`(a, b)` の一つの綴りしかない -- `[...]` は撤去された",
        "@memo(xs: (1, 2, 3))\n",
    ),
    case(
        None,
        "空の列と一要素の列。値に中置演算はないので、括弧は何もまとめない。\
         `(x)` はカンマがなくても一要素の列",
        "@memo(a: (), b: (x))\n",
    ),
    case(None, "入れ子の列", "@memo(xs: ((a), (b, c), ()))\n"),
    case(
        None,
        "型の `list(string)` は列ではなく呼び出しのまま残る",
        "::field(tags: { type: list(string) })\n",
    ),
    case(None, "入れ子のマップ", "@memo(m: { x: 1 })\n"),
    case(
        None,
        "値位置の `|` は括弧のない `[ ... ]`。散文の `,` や `}` では閉じず、\
         `|` で始まらない次の行が次のキーになる",
        "@memo(\n  a: | x, y }\n     | z\n  b: 2\n)\n",
    ),
    case(None, "引用符つき文字列", "@memo(s: \"a, b: c\")\n"),
    case(
        None,
        "引用符つきのキー",
        "@memo(\"a/b.tmt\": 1, plain: 2)\n",
    ),
    case(
        None,
        "スカラーは型が推論される",
        "@memo(i: 1, f: 1.5, b: true, n: null)\n",
    ),
    case(
        None,
        "同じ種類のグループを続けて書くとエラー。\
         `:` を前に置けば、既にあるほうへマージされる",
        "@memo(a: 1)(b: 2)\n",
    ),
    case(
        None,
        "空白を挟めば地の文の括弧。`@file(x) (説明)` が壊れないための線引き",
        "@memo(a: 1) (b: 2)\n",
    ),
    // ---- `|content` --------------------------------------------------
    case(
        Some("`|` で開く内容"),
        "`|` は `[` が置ける位置に置ける。`]` の代わりに、\
         `|` で始まる行の連なりの終わりで閉じる",
        "@quote(A)\n| 一行目\n| 二行目\n",
    ),
    case(
        None,
        "同じものを括弧で書いた形。木は一致する",
        "@quote(A)[ 一行目\n  二行目 ]\n",
    ),
    case(None, "要素の行で開いてもよい", "@quote| 本文\n"),
    case(
        None,
        "リスト項目も例外ではない。継続行は開いた `|` と同じ列に立つ",
        "- (x)| 一行目\n     | 二行目\n",
    ),
    case(None, "見出しも同じ", "#| 一行目\n | 二行目\n"),
    case(
        None,
        "継続する先が無い `|` は地の文。地の文にエスケープが無いので、\
         行頭の `|` は綴れなければならない",
        "| a | b |\n",
    ),
    // ---- `\` continuation ---------------------------------------------
    case(
        Some("`\\` による段落継続"),
        "既定は孤立。行頭の `\\` は、直前のブロックへ畳み込んで段落を続ける",
        "@link(target: \"https://a.example\")[a]\n\\@link(target: \"https://b.example\")[b]\n",
    ),
    case(
        None,
        "行末の `\\` でも同じ木になる",
        "@link(target: \"https://a.example\")[a] \\\n@link(target: \"https://b.example\")[b]\n",
    ),
    case(
        None,
        "継ぎ目を一度越えれば、以降は `\\` なしで続く",
        "@link(target: \"https://a.example\")[a]\n\
         \\@link(target: \"https://b.example\")[b]\n\
         @link(target: \"https://c.example\")[c]\n",
    ),
    case(
        None,
        "冗長な `\\` は黙って許容される",
        "@link(target: \"https://a.example\")[a] \\\n\
         \\@link(target: \"https://b.example\")[b] \\\n\
         \\@link(target: \"https://c.example\")[c]\n",
    ),
    case(
        None,
        "`\\` を書かなければ、今まで通り別々のブロックに割れる",
        "@link(target: \"https://a.example\")[a]\n@link(target: \"https://b.example\")[b]\n",
    ),
    case(
        None,
        "パーサーは種類を知らずに `\\` を解釈するので、既に解析済みの\
         どんな `Block::Element`（水平線など）も同じように畳み込める。\
         ただし `hr` は `required_shape` が常に `Block` なので、\
         `tomet check` の `shape_mismatch` に引っかかる -- 稀にしか\
         起きない、明示的な誤用でしかない",
        "text\n\n---\n\\joined\n",
    ),
    case(
        None,
        "継ぐ先が無い `\\` は黙って消費される（構文エラーにはならない）",
        "\\@link(target: \"https://a.example\")[dangling]\n",
    ),
    // ---- lists ------------------------------------------------------
    case(
        Some("リスト"),
        "`-` が非順序、`-.` が順序",
        "- ひとつ\n- ふたつ\n",
    ),
    case(None, "順序つき", "-. ひとつ\n-. ふたつ\n"),
    case(
        None,
        "グループが直接続けば空白は要らない。`@name[ x ]` と同じ",
        "-[ 括弧 ]\n-{ id: x }\n-| マーカー\n",
    ),
    case(
        None,
        "`(...)` だけの項目。本文が続かなくてもマーカーになる。\
         `@name(id: a)` が引数つきの要素になるのと同じ",
        "- ( )\n-(x)\n- (y)\n",
    ),
    case(
        None,
        "`--` は**ネストしない**。2行目は段落になる（未対応）",
        "- 親\n-- 子\n",
    ),
    case(
        None,
        "`(12:01)` は `{12: 1}` に**誤って**分解される。`:` が \
         `key:value` として読まれるため",
        "- (12:01) 本文 {id: a}\n",
    ),
    case(
        None,
        "末尾の `{...}` が `:` なしで項目に付く",
        "- 本文 {id: a}\n",
    ),
    // ---- inline markup ----------------------------------------------
    case(
        Some("インライン記法"),
        "強調・太字・マーク・取り消し線・コード",
        "*em* と **strong** と ==mark== と ~~strikeout~~ と `code`\n",
    ),
    case(None, "自動リンク", "見て https://example.com/x ください\n"),
    // ---- breaks and code --------------------------------------------
    case(Some("区切りとコードブロック"), "区切り線", "---\n"),
    case(None, "見出しつき区切り線", "---[ 章題 ]---\n"),
    case(
        None,
        "``` フェンス。中身は逐語",
        "```rust\nlet y = @T; *ptr\n```\n",
    ),
    // ---- interpolation ----------------------------------------------
    case(Some("補間 `${...}`"), "識別子", "${name}\n"),
    case(None, "メンバ参照", "${a.b.c}\n"),
    case(None, "関数呼び出し", "${sum(1, 2)}\n"),
    case(None, "`$name(...)` 形式", "$uuid()\n"),
    case(Some("式の部品"), "`true` `false` `null` は値", "${true}\n"),
    case(None, "`null`", "${null}\n"),
    case(
        None,
        "比較は連鎖しない。`==` `!=` `<` `<=` `>` `>=`",
        "${a >= 1}\n",
    ),
    case(None, "`&&` は `||` より強く結びつく", "${a || b && c}\n"),
    case(None, "括弧は木に残る", "${(a || b) && c}\n"),
    case(None, "無名関数", "${xs.filter(d => d.n == 1)}\n"),
    case(
        None,
        "引数が複数の無名関数は括弧で囲む",
        "${(a, b) => a < b}\n",
    ),
    case(
        None,
        "`#` の左に文書。左を省くと `self`",
        "${tmt(\"a.tmt\")#intro.prop}\n",
    ),
    case(
        None,
        "`addr(` は中身を生の文字列として読む",
        "${addr(tmt:docs/a.tmt#intro)}\n",
    ),
    // ---- connect ----------------------------------------------------
    case(
        Some("コネクト `::`"),
        "`::{...}` は値をマージ",
        "@task[ A ]::{ id: t1 }\n",
    ),
    case(None, "`::(...)` は args をマージ", "@task(a: 1)::(b: 2)\n"),
    case(
        Some("名前付きコネクト `::name(...)`"),
        "`::` の直後に名前があれば、マージではなく別の要素として `connects` に積まれる。\
         現時点で唯一のメンバーは `rule`",
        "@section[ x ]::rule(allow: (card))\n",
    ),
    case(
        None,
        "`::id(x)` は所有者の `#(x)`、`::meta(a: 1)` は `{a: 1}` を、そのまま書き下した形。\
         どちらも `connects` には積まれず、所有者のidと値になる",
        "@x(a: 1)::id(i1)::meta(k: v)\n",
    ),
    case(
        None,
        "複数のコネクトを重ねられる。それぞれ独立した `connects` の要素になる",
        "@x(a: 1)::as(y)::rule(allow: (card))\n",
    ),
    case(
        None,
        "`string`/`int`/`float` は即座に確定するリテラル呼び出しで、パーサが値に解決する。\
         それ以外の呼び出し名(型の `list(string)` を含む)は、コネクトの名前と同じく\
         パーサは判断せず、そのまま残す。列は呼び出しではなく裸の `(a, b)`",
        "@x(a: (card, ns.mycard))\n",
    ),
    case(
        None,
        "リスト項目・見出しの糖衣形でも bare connect が使える。`::()`/`::{}`は\
         どちらが先でもよく、両方書いても一つの `:` で足りる",
        "- content ::(x: 1){y: 2}\n",
    ),
    case(None, "見出しの糖衣形も同様", "= title ::(x: 1){y: 2}\n"),
    case(
        None,
        "改行1つまでなら次の行に置いてもよい(他のsigilの groups と同じ gap)。\
         空行を挟む(改行2つ)とつながらない -- REJECTEDの同じ例を参照",
        "- content\n::{y: 2}\n",
    ),
    case(
        None,
        "リスト項目は named connect も直接持てる。完全形・糖衣形のどちらでも、\
         要素自身ではなく項目自身の `connects` に積まれる",
        "- [ x ]::rule(allow: (card))\n",
    ),
    case(
        None,
        "見出しの糖衣形の named connect も、完全形(`=[ h ]::rule(...)`)と同じ扱い",
        "= h ::rule(allow: (card))\n",
    ),
    // ---- id `#(...)` --------------------------------------------------
    case(
        Some("id `#(...)`"),
        "要素自身のid。`(args)[content]{value}`の後、コネクトの前に読まれる",
        "@task(a: 1)[ text ]#(myid)\n",
    ),
    case(
        None,
        "groupsより前に書いても同じ。idと他のgroupsに順序はない",
        "@task#(myid)(a: 1)[ text ]\n",
    ),
    case(
        None,
        "クォートすればスペースも入る",
        "@task()#(\"has spaces\")\n",
    ),
    case(None, "セクションにも付く", "=[ Intro ]#(intro)\n"),
    case(None, "リスト項目の完全形にも付く", "- (x)[ y ]#(item1)\n"),
    case(
        None,
        "糖衣形は末尾の`#(id)`と`{attrs}`を両方読む",
        "- plain text #(item1){ tag: x }\n",
    ),
    case(
        None,
        "コネクトは`#(id)`を取らない。コネクトの直後に書くとエラー\
         (idは所有者のものなので、最初のコネクトより前に書く)",
        "@task(a: 1)::rule(allow: (card))#(myid)\n",
    ),
    case(
        None,
        "所有者のidは最初のコネクトより前に書く",
        "@task(a: 1)#(myid)::rule(allow: (card))\n",
    ),
    case(None, "idを2つ書くとエラー", "@task()#(a)#(b)\n"),
    // ---- comments ---------------------------------------------------
    case(Some("コメント"), "行コメント", "// 消える\n本文\n"),
    case(None, "ブロックコメント", "本文 /* 消える */ の続き\n"),
    // ---- document declarations --------------------------------------
    case(
        Some("文書の宣言"),
        "行頭の `::名前` は、文書自身についての宣言。`@名前` と同じ groups を取り、\
         先頭の並び（preamble）に置く。`(args)` か `[content]` が要る。名前は構文では選ばない（判定は検証の仕事）",
        "::kind(note)\n::meta(id: x)\n",
    ),
    case(
        None,
        "groups の全部（`(args)` `[content]` `{value}` `#(id)`）を取る。本文を取れるかは語彙が決める",
        "::x(a)[ b ]{ c: 1 }#(i)\n",
    ),
    case(
        None,
        "`{...}` だけを取っていた12個の宣言は `(...)` を取る: \
         `meta` `macros` `format` `fields` `workspace` `export` `blueprints` \
         `vocabularies` `dictionaries` `migration` `vars` `elements`。\
         `::meta{ ... }` はもう書けない（下の「主グループの無いネクサス」）",
        "::meta(title: \"x\")\n::macro(gh: \"https://x/${1}\")\n::var(a: { type: string })\n\
         ::format(table: { max_col_width: 50 })\n::field(created: { type: datetime })\n\
         ::workspace(ignore: (\"a\"))\n::export(type: commonmark)\n::blueprint(b: \"b.tmt\")\n\
         ::vocabulary(v: \"v.tmt\")\n::dictionary(d: \"d.tmt\")\n::migration(m: 1)\n\
         ::element(card: { display: block })\n",
    ),
    case(
        None,
        "`::vocabulary(ns){ ... }` と `::blueprint(kind){ ... }` はもともと `(...)` を持つので、\
         `{...}` の本体をそのまま取る。辞書の語は本文のブロック（`;@term(\"語\")[説明]`）で、\
         `::dictionary(name)` は `{...}` を取らない",
        "::vocabulary(ns){ version: \"1.0.0\" }\n::blueprint(note){ version: \"1.0\" }\n\
         ::dictionary(tomet)\n\n;@term(\"vault\")[ a unit ]\n",
    ),
    case(None, "中身が空の宣言も `()` を書く", "::meta()\n"),
    case(
        None,
        "次の行の `::` は、前の宣言のコネクトではなく、次の宣言",
        "::kind(note)\n::use(deck)\n",
    ),
    case(
        None,
        "行頭でない `::` は、ただの文字（`std::vec`）。名前が無い `::(x)` も文字",
        "std::vec と ::kind(x) と\n\n::(x)\n",
    ),
    // ---- footnotes --------------------------------------------------
    case(
        Some("脚注"),
        "`@ftn(id)[文]` が定義、内容の無い `@ftn(id)` が参照、`@ftn[文]` はその場かぎりの脚注",
        "参照@ftn(a) と その場かぎり@ftn[文]\n\n@ftn(a)[定義]\n",
    ),
];

/// Constructs that were retired, are easy to misread, or are rejected.
///
/// Not all of these fail to parse -- and that is deliberate. A construct
/// that was *supposed* to be removed but still parses is exactly the kind
/// of gap this report exists to surface, so it is listed with whatever it
/// actually does rather than with what it was meant to do.
const REJECTED: &[Case] = &[
    case(
        Some("主グループの無いネクサス"),
        "ブロックの `@名前` だけの行は要素ではなくエラー。空の要素は `@名前()` と書く",
        "@memo\n",
    ),
    case(None, "水平線も同じ。`@hr` ではなく `@hr()`", "@hr\n"),
    case(None, "`{...}` だけでは要素にならない", "@memo{ a: 1 }\n"),
    case(None, "空の `{}` も同じ", "@memo{}\n"),
    case(
        None,
        "`#(id)` だけでも要素にならない。id は要素に付くもので、要素を作らない",
        "@memo#(first)\n",
    ),
    case(
        None,
        "付加のグループを重ねても主グループにはならない",
        "@memo{}#(first)\n",
    ),
    case(
        None,
        "名前つきコネクトだけでは要素にならない",
        "@memo::rule(allow: (card))\n",
    ),
    case(
        None,
        "名前のない `::{...}` も、主グループのない `@名前` には付けられない",
        "@memo::{ a: 1 }\n",
    ),
    case(None, "`::(...)` も同じ", "@memo::(a: 1)\n"),
    case(
        None,
        "コネクトも主グループを要る。`::rule{...}` は書けない",
        "@memo()::rule{ allow: (card) }\n",
    ),
    case(
        None,
        "リスト項目のコネクトも同じ",
        "- 項目 ::rule{ allow: (card) }\n",
    ),
    case(
        None,
        "見出しのコネクトも同じ",
        "= 見出し ::rule{ allow: (card) }\n",
    ),
    case(None, "`-@名前` も主グループを要る", "-@memo{ a: 1 }\n"),
    case(
        None,
        "行の途中の `@名前` は、グループが無ければ地の文（上の「文字列に落ちる場合」）。\
         しかし付加のグループが続くと、地の文にはならずエラー",
        "この @memo{ a: 1 } は書きかけ\n",
    ),
    case(
        None,
        "`#(id)` が続く場合も同じ",
        "この @memo#(first) は書きかけ\n",
    ),
    case(
        None,
        "宣言も同じ。`::名前` だけの行は宣言ではなくエラー。空の宣言は `::meta()`",
        "::kind(note)\n::meta\n",
    ),
    case(
        None,
        "`{...}` だけを取っていた宣言の旧い書き方。`::meta{ ... }` は `::meta(...)` になった",
        "::kind(note)\n::meta{ title: \"旧い書き方\" }\n",
    ),
    case(
        None,
        "12個の宣言のどれも同じ。`::format{ ... }` も受け付けない",
        "::format{ table: { max_col_width: 50 } }\n",
    ),
    case(
        None,
        "宣言も `#(id)` だけでは宣言にならない",
        "::kind(note)\n::meta#(first)\n",
    ),
    case(
        Some("宣言の制限"),
        "宣言はコネクトを取らない。コネクトは要素に付くもので、宣言は何にも付かない",
        "::kind(note)::rule(allow:(k))\n",
    ),
    case(
        Some("撤去された構文"),
        "`<T>` シジルは廃止。ただの文字列になる",
        "<memo>[ x ]\n",
    ),
    case(
        None,
        "名前なしの `@` も撤去。`@(url:)` の推論は `1d0b7b2` の \
         `infer.rs` 削除で消え `@link(target:)` に統一されたのに、 \
         パーサだけが構文を受け付け続けていた",
        "@(url: \"https://example.com\")[リンク]\n",
    ),
    case(None, "`@[ ... ]` も同じ", "@[ x ]\n"),
    case(
        None,
        "`[a, b]` 列リテラルも撤去。`[`/`]` は既に `[content]` の意味を\
         持つので、値の中の列は `(a, b)` の一つの綴りしかない",
        "@memo(xs: [1, 2, 3])\n",
    ),
    case(
        Some("単一コロンのコネクト"),
        "`:name(...)` は撤去された。`:` は `key: value` の区切りだけを担い、\
         コネクトは `::name(...)`。エラーは書くべき形を示す",
        "@x(a):rule(allow: (card))\n",
    ),
    case(
        None,
        "名前のないコネクトも同じ。`::{...}` と書く",
        "@x(a):{ b: 1 }\n",
    ),
    case(
        None,
        "行頭の `::name` は前の行の要素のコネクトではなく、宣言。\
         要素のあとに宣言を書くと、宣言は文書の先頭に置くものというエラー",
        "@x(a)\n::rule(allow: (card))\n",
    ),
    case(
        None,
        "列は値だけを持ち、`key: value` の項目は持てない。マップは `{...}`",
        "@memo(xs: (a: 1))\n",
    ),
    case(
        None,
        "データの列を `list(...)` とは書かない。呼び出しとして残り、\
         列として読む側（`ignore:` など）が受け付けない",
        "::workspace(ignore: list(\"a\"))\n",
    ),
    case(
        None,
        "`#name` のブロックシジルも撤去。形はシジルではなく位置が決める。\
         `#` 自体も見出しの糖衣とタグの糖衣を失い、今は完全に自由な文字",
        "#memo[ x ]\n",
    ),
    case(
        Some("綴りを失ったまま、代わりが未決のもの"),
        "リモート接続。`<id:taskA>:{...}` と書いていたが `<T>` と共に \
         失われた。代わりの書き方は決まっていないので、実装も \
         受け付けない",
        "@id(taskA)::{ priority: high }\n",
    ),
    case(
        Some("紛らわしいが、これが正しい"),
        "`(content:raw)` は普通の引数。`[...]` の解釈を変えない",
        "@memo(content:raw)[\n1行目\n2行目\n]\n",
    ),
    case(
        None,
        "sugar bodyのtrailing connectは改行1つまで。空行(改行2つ)を挟むと\
         要素につながらず、`::{...}`はただの地の文になる",
        "- content\n\n::{ y: 2 }\n",
    ),
    case(
        Some("受け付けない書き方"),
        "`{...}` に裸のスカラーも書けない（裸の値は `(args)` に書く）",
        "@memo(){hello}\n",
    ),
    case(None, "閉じない `[`", "@memo[ 閉じない\n"),
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

    match parse_file(case.src) {
        Ok(doc) => {
            let _ = write!(out, "```\n{}```\n", dump_file(&doc));
            // Parsing and validating are separate on purpose: the parser
            // consults no element vocabulary, so "is this a real element"
            // is only answerable here.
            let errors = tomet_validator::validate_document(&doc);
            if !errors.is_empty() {
                out.push_str("\n検証:\n\n```\n");
                for e in &errors {
                    let _ = writeln!(out, "{e}");
                }
                out.push_str("```\n");
            }
        }
        Err(e) => {
            let _ = write!(out, "```\nparse error: {e}\n```\n");
        }
    }
    // A case in the rejected half that parses anyway is worth calling
    // out: either it means something other than it looks, or it was meant
    // to be gone and is not.
    if rejected_section && parse_file(case.src).is_ok() {
        out.push_str("\n> **パースは通る。** 上が実際の結果。\n");
    }
}

fn anchor(section: &str) -> String {
    section
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '`' | '(' | ')' | '{' | '}' | '[' | ']' | '.' | '+' | '$' | '#' | '@' | ':' | '|'
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
// `{:?}` on a `File` is unreadable here: every node carries a `Span`
// with three fields, which is most of the output and none of the meaning.
// This prints the shape only.

fn dump_file(file: &File) -> String {
    let mut out = String::new();
    for decl in &file.header {
        dump_declaration(&mut out, decl, 0);
    }
    for block in &file.body {
        dump_block(&mut out, block, 0);
    }
    out
}

fn dump_declaration(out: &mut String, declaration: &Declaration, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "Declaration ::{}", declaration.name);
    dump_groups(
        out,
        &declaration.data,
        declaration.content.as_deref(),
        depth,
    );
    if let Some(id) = &declaration.id {
        indent(out, depth + 1);
        let _ = writeln!(out, "id      {:?}", id.0);
    }
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
        Block::Element(el) => dump_element(out, el.as_view(), depth),
        Block::Interp(interp) => dump_interp(out, interp, depth),
        Block::Section(sec) => {
            indent(out, depth);
            let _ = writeln!(out, "Section level={}", sec.level);
            indent(out, depth + 1);
            out.push_str("Title:\n");
            for inline in &sec.title {
                dump_inline(out, inline, depth + 2);
            }
            if let Some(args) = &sec.data.args {
                indent(out, depth + 1);
                let _ = writeln!(out, "args    {}", value_str(args));
            }
            match &sec.data.value {
                Some(Group(entries)) => {
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
                            Entry::Element(child) => dump_element(out, child.as_view(), depth + 2),
                            Entry::Hole(child) => dump_hole(out, child, depth + 2),
                            Entry::Bare(child) => dump_bare(out, child, depth + 2),
                        }
                    }
                }
                None => {}
            }
            if let Some(id) = &sec.id {
                indent(out, depth + 1);
                let _ = writeln!(out, "id      {:?}", id.0);
            }
            for connect in &sec.connects {
                indent(out, depth + 1);
                out.push_str("connect\n");
                dump_connect(out, connect, depth + 2);
            }
            if !sec.blocks.is_empty() {
                indent(out, depth + 1);
                out.push_str("Blocks:\n");
                for child in &sec.blocks {
                    dump_block(out, child, depth + 2);
                }
            }
        }
        Block::List(list) => dump_list(out, list, depth),
        Block::Hole(hole) => dump_hole(out, hole, depth),
    }
}

fn dump_list(out: &mut String, list: &List, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "List ordered={}", list.ordered);
    if let Some(id) = &list.id {
        indent(out, depth + 1);
        let _ = writeln!(out, "id      {:?}", id.0);
    }
    for connect in &list.connects {
        indent(out, depth + 1);
        out.push_str("connect\n");
        dump_connect(out, connect, depth + 2);
    }
    indent(out, depth + 1);
    out.push_str("Items:\n");
    for item in &list.items {
        dump_item(out, item, depth + 2);
        if let Some(sub) = &item.sublist {
            indent(out, depth + 2);
            out.push_str("sublist\n");
            dump_list(out, sub, depth + 3);
        }
    }
}

fn dump_inline(out: &mut String, inline: &Inline, depth: usize) {
    match inline {
        Inline::Text(t) => {
            indent(out, depth);
            let _ = writeln!(out, "Text {:?}", t.value);
        }
        Inline::Raw(t) => {
            indent(out, depth);
            let _ = writeln!(out, "Raw {:?}", t.value);
        }
        Inline::SoftBreak(_) => {
            indent(out, depth);
            let _ = writeln!(out, "SoftBreak");
        }
        Inline::LineBreak(_) => {
            indent(out, depth);
            let _ = writeln!(out, "LineBreak");
        }
        Inline::Element(el) => dump_element(out, el.as_view(), depth),
        Inline::Interp(interp) => dump_interp(out, interp, depth),
        Inline::Hole(hole) => dump_hole(out, hole, depth),
    }
}

/// A hole's head line reads like an element's: its placement, then `?` and its matcher.
fn dump_hole(out: &mut String, hole: &Hole, depth: usize) {
    let placement = match hole.placement {
        Placement::Block => "Block ",
        Placement::Inline => "Inline",
    };
    let matcher = hole
        .matcher
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    indent(out, depth);
    let _ = writeln!(out, "{placement} ?{matcher}");
    dump_groups(out, &hole.data, hole.content.as_deref(), depth);
}

fn dump_interp(out: &mut String, interp: &Interpolation, depth: usize) {
    indent(out, depth);
    out.push_str("Interp $\n");
    indent(out, depth + 1);
    let _ = writeln!(out, "interp  {}", interp_str(&interp.expr));
}

fn dump_element(out: &mut String, el: ElementView<'_>, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "{}", element_head(el));

    dump_groups(out, el.data, el.content, depth);
    if let Some(id) = el.id {
        indent(out, depth + 1);
        let _ = writeln!(out, "id      {:?}", id.0);
    }
    for connect in el.connects {
        indent(out, depth + 1);
        out.push_str("connect\n");
        dump_connect(out, connect, depth + 2);
    }
}

/// A list item reads as the element it is when it has a name, and as a bare one
/// when it has not.
fn dump_item(out: &mut String, item: &ListItem, depth: usize) {
    if let Some(element) = item.as_element_view() {
        dump_element(out, element, depth);
        return;
    }
    indent(out, depth);
    out.push_str("Bare\n");
    dump_groups(out, &item.data, Some(&item.content), depth);
    if let Some(id) = &item.id {
        indent(out, depth + 1);
        let _ = writeln!(out, "id      {:?}", id.0);
    }
    for connect in &item.connects {
        indent(out, depth + 1);
        out.push_str("connect\n");
        dump_connect(out, connect, depth + 2);
    }
}

/// A nameless `{...}` entry's head line is `Bare`, as it always was.
fn dump_bare(out: &mut String, bare: &Bare, depth: usize) {
    indent(out, depth);
    out.push_str("Bare\n");
    dump_groups(out, &bare.data, bare.content.as_deref(), depth);
    if let Some(id) = &bare.id {
        indent(out, depth + 1);
        let _ = writeln!(out, "id      {:?}", id.0);
    }
}

/// A connect's head line reads like an inline element's, as it always did.
fn dump_connect(out: &mut String, connect: &Connect, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "Inline @{}", connect.name);
    dump_groups(out, &connect.data, connect.content.as_deref(), depth);
}

/// `(args)`, `[content]` and `{value}`, the groups a node has in common.
fn dump_groups(out: &mut String, data: &Data, content: Option<&[Block]>, depth: usize) {
    if let Some(args) = &data.args {
        indent(out, depth + 1);
        let _ = writeln!(out, "args    {}", value_str(args));
    }
    if let Some(content) = content {
        indent(out, depth + 1);
        out.push_str("content\n");
        for block in content {
            dump_block(out, block, depth + 2);
        }
    }
    match &data.value {
        Some(Group(entries)) => {
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
                    Entry::Element(child) => dump_element(out, child.as_view(), depth + 2),
                    Entry::Hole(child) => dump_hole(out, child, depth + 2),
                    Entry::Bare(child) => dump_bare(out, child, depth + 2),
                }
            }
        }
        None => {}
    }
}

/// The element's head line in the dump: its placement, then its name.
///
/// Placement is shown because the name does not carry it -- every element is
/// `@name`, and whether it stands as a block is decided by position.
fn element_head(el: ElementView<'_>) -> String {
    let placement = match el.placement {
        Placement::Block => "Block ",
        Placement::Inline => "Inline",
    };
    format!("{placement} @{}", el.name)
}

fn value_str(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        // Kept as written; named, so a report shows it was not read as a float.
        Value::Number(text) => format!("number({text})"),
        Value::String(s) => format!("{s:?}"),
        // Written as the source spells it: `[...]` is block content here
        // (`Value::Blocks` below), not a list.
        Value::Seq(items) => {
            let inner: Vec<String> = items.iter().map(value_str).collect();
            format!("({})", inner.join(", "))
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
        Value::Call(name, args) => {
            let inner: Vec<String> = args.iter().map(value_str).collect();
            format!("{name}({})", inner.join(", "))
        }
        Value::Interp(interp) => format!("${{{}}}", interp_str(&interp.expr)),
        Value::Hole(hole) => {
            let matcher = hole
                .matcher
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            match &hole.data.args {
                Some(args) => format!("?{matcher}({})", value_str(args)),
                None => format!("?{matcher}"),
            }
        }
        Value::Element(el) => {
            let name = el.name.to_string();
            match &el.data.args {
                Some(args) => format!("@{name}({})", value_str(args)),
                None => format!("@{name}"),
            }
        }
        Value::Blocks(blocks) => {
            let mut inner = String::new();
            for block in blocks {
                dump_block(&mut inner, block, 0);
            }
            format!("[{}]", inner.trim())
        }
    }
}

fn interp_str(expr: &InterpExpr) -> String {
    use tomet_ast::{InterpExprKind as K, Literal};
    match &expr.kind {
        K::Identifier(name) => name.clone(),
        K::Id(name) => format!("#{name}"),
        K::Literal(Literal::Int(i)) => i.to_string(),
        K::Literal(Literal::Float(f)) => f.to_string(),
        K::Literal(Literal::String(s)) => format!("{s:?}"),
        K::Literal(Literal::Bool(b)) => b.to_string(),
        K::Literal(Literal::Null) => "null".to_string(),
        K::Call { callee, args } => {
            let inner: Vec<String> = args.iter().map(interp_str).collect();
            format!("{}({})", interp_str(callee), inner.join(", "))
        }
        K::Member { object, member } => format!("{}.{member}", interp_str(object)),
        K::NamedArg { name, value } => format!("{name}: {}", interp_str(value)),
        K::Binary { op, lhs, rhs } => {
            format!("({} {} {})", interp_str(lhs), op.symbol(), interp_str(rhs))
        }
        K::Group(inner) => format!("({})", interp_str(inner)),
        K::DocId { doc, id } => format!("{}#{id}", interp_str(doc)),
        K::Addr { text, .. } => format!("addr({text})"),
        K::Lambda { params, body } => {
            format!("lambda({}) => {}", params.join(", "), interp_str(body))
        }
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
