<!-- Generated from tmtroot/locales/zh-Hans/readme.tmt. Edit that, then `tomet export .`. -->

> [!warning]
> Tomet 目前正在开发中，语法和规范每天都在发生很大变化。
> 试用时请先做好备份，不要用于正式运行。

> [!note]
> 本页是尚未经过人工确认的翻译草稿。内容以英文版 README 为准。

# Tomet

[English](../../README.md) | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

Tome to me!!

## Tomet 是什么？

1. 符合人体工学，易于阅读。
2. 对解析器同样友好。
3. 扩展性有保障。
4. 重视 key:value。
5. 出于研究目的而开发。

## 为什么使用 Tomet？

1. 无歧义的语法
2. 专注于链接与变量
3. 统一的记法
4. 为文字标注读音（ruby）很容易
5. 保留字面量

> [!note]
> 适合希望像 MDX 或 Astro 组件那样，在文档中嵌入带类型的数据或组件的工程师
> 适合希望像 Obsidian 的 Dataview 或 Bases 那样，将元数据与笔记深度联动的知识库构建者

## 示例

```tmt
::id(readme)

=[ heading ]

- list #(id)
  -. orderd

@table
|[ col1 ][ col2 ][ col3 ]

@callout(info)
| content
```

## 已知的弱点

1. 视觉噪音与输入负担。符号过多（因此重视 LSP 功能）
2. 与 Markdown 并不完全兼容

## 支持的编辑器

- Helix
- Neovim
- VSCode
- Zed

