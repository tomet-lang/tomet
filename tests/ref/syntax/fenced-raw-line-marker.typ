= marked fence lines

コードフェンス（\`\``）の内部では、行頭の `\` は行マーカーとして扱われる。行マーカーの直後の半角空白1つは区切りとして落とされ、2つ目以降の空白は本文に残る。

```(rust)
fn main() {
    println!("hello");
}
```

= empty lines

マーカーのみの行（\`\`）および空行はいずれも空行として扱われる。

```(text)
first


last
```

= content starting with backticks

本文自体が \`` で始まる行は、`` `\` と記述する。また、マーカー行の中にある \`\`\` はフェンスを閉じない。

````(tmt)
`` line raw sample
```
code continues
````

