# `{...}` の中で、裸のエントリだけ `{value}` を取れない

**状態:** 未着手。調査済み。`-[ x ]` と同種の非対称。

## 実測

```
@links{ @param(id){x: 1} }        → ok      名前つき要素は {value} を取れる
@links{ @param(id)[a]{x: 1} }     → ok
@links{ (1)[a]{y: 2} }            → ERROR   裸のエントリは取れない
@links{ (1){y: 2} }               → ERROR
```

エラー文言:

```
a '{...}' group holds 'key: value' entries or elements;
write a bare value in '(args)', or use a '+++' fence
```

しかも位置が `{` を指しているので、どのエントリが原因か分からない。

## 何が食い違っているか

`crates/tomet-syntax-parser/src/element.rs` の `parse_value_group` の doc comment は
こう書いている:

> "A nested element" means a *named* one too, not only the bare
> `(marker)[content]` form.

つまり裸のエントリの想定形が `(marker)[content]` で止まっていて、`{value}` が
入っていない。名前つきは `parse_element` → `parse_groups` を通るので三つ全部
取れる。**同じ「要素」が、シジルの有無で取れるグループの数を変えている。**

これは `181b542` で直した `-[ x ]`（`-` だけ `(` 以外の開き括弧を受けない）と
同じ species。認識する場所が複数あって、片方だけ更新されていない。

## 影響

`colon-as-sigil.md` の規則検証がここで止まった。`:` の行き先を決める規則を
`{...}` グループという容れ物で試すには「value が埋まった裸エントリのあとに
`:{}`」が要るが、その形が書けない。**この非対称が解けるまで、あの容れ物に
ついては何も言えない。**

## 案

`parse_value_group` の裸エントリ経路を `parse_groups` に通す。`-` と `#` を
`parse_groups` に一本化したのと同じ手（`2a99315`）。`sigil-follow-set.md` の
「シジルごとに認識が 2 箇所ある」構造の 4 例目にあたるので、あちらと一緒に
片付くかもしれない。

着手前に確認すること:

- 裸エントリが `{value}` を持てると `@links{ (1)[a]{y:2} (2)[b] }` の区切りが
  曖昧にならないか。`{y:2}` がエントリのものか次のエントリの前の孤立した
  グループかは、`(` が来るまで分からない。
- エラー位置が `{` を指しているのも直す。原因のエントリを指すべき。
