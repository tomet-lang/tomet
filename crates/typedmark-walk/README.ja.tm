Documentの木構造を辿る処理だけを提供する

typedmark-astは型定義だけ、typedmark-semanticsは「1つのElementが何を意味するか」の判定だけ、で、どちらも「Document全体をどう再帰的に辿るか」というロジックは持っていない。typedmark-validator(重複id検出)とtypedmark-resolve(`${id}`参照解決)が、それぞれ独立に同じ形の木構造ウォーカーを書いていたのを見つけて、ここに切り出した。typedmark-astにしか依存しないので、どのconsumerも余計な依存(I/Oなど)を引きずらずに使える。

#[ 実装済み ]

- walk_document(doc, visitor): Heading/ListItem/Elementを再帰的に訪問(contentの中、ElementValue::Childrenの中も含む)
- Visitorトレイト: visit(Node)を実装するだけ。ControlFlow::Breakで即座に止められる(resolveのような「最初の一致で止めたい」用途向け)、常にContinueを返せば全部集められる(validatorのような「全部集めたい」用途向け)
- Node: Heading/ListItem/Elementを統一。`.attrs()`でid探索に使うValue::Mapを、`.span()`で位置を取れる
