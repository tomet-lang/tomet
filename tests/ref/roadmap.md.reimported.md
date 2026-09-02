- testファイルを別途用意しろ。
- ちゃんと分割しろ
- 読み込みロード画面

## 構文を決定する

- \`ref\`構文
- エスケープ構文
- Template構文
- \`\[\]\`のkey
- コネクト構文2
- Table記法
- 独自DSL Settings
- 独自DSL Value

## 構文を実装する

- ビルトイン関数の強化
- Elementの挙動を一般化
- \`#meta\`にkdlを追加

## 拡張性を確保する

- ドキュメントの補充
- アイコン、ロゴ
- IDEサポート

## CLI/Library機能を強化する

- 置換/検索
- 変換を強化
- TUIエディター
- Webエディター
- Svelte向けパッケージ
- React向けパッケージ
- Hook

## 外部機能を強化する

- Zed
  - フォーマッタ
  - サジェスト
  - 自動補完
- 多言語バインディング
- その他

# Archive

## 構文

- 要素設定 (.settings.tmt, json, yaml)
- コネクト構文 (\`:(){}\`)
- \`args\`推論 (基本要素対応, カスタム要素対応, positional: \[ title, priority \])
- \`\`@\`\`推論
- 代入、関数構文 (\`\`$\`{var}\` \`compute\`,\`resolve\`)
- 構文エラー (重複)
- コメントアウト
- 公式構文 (\`#settings\`, \`#config\`, \`#meta\`, \`#references\`)
- 基本構文 (空白可, 改行可, 順不同)
- 基本Markdown記法 (\`\*\`, \`\*\*\`, \`==\`, \`\_\_\`, \`\`)
- 基本要素 (heading, list, horizontal line)
- 名前 (element, arg, content, value)
- 基本AST (\`\`@\`T()\[\]{}\`)

## 機能

- to\_from\_markdown ()
- export (\`export:{type: commonmark, path: ...}\`)

