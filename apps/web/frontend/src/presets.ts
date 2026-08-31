export const presets: Record<string, string> = {
	cheatsheet: `@meta(format: json){
  {
    "title": "Tomet Playground",
    "tags": ["markup", "rust", "wasm", "svelte", "react"]
  }
}

@config{
  macros: {
    gh: "https://github.com/tomet/tomet/issues/\${1}"
    greet: "Hello, \${1} \${2}!"
    copyright: "(C) 2026 Tomet Projects"
  }
}

#[ Tomet (TM) Interactive Playground ]

<callout>(type: info)[
  Tomet combines pure static deterministic syntax with rich extensible elements.
]

##[ 1. Explicit Links & References ]

- Official Repository: @link("https://github.com/tomet/tomet")[Tomet GitHub]
- Documentation: @link("https://github.com/tomet/tomet/tree/main/docs")[Docs]
- Same-document anchor: @link(id: "sprint-tasks")[Jump to Tasks]

##[ 2. Tasks & Status Lists ]{id: sprint-tasks}

- ( ) Task 1: Unchecked item
- (x) Task 2: Completed item
- (T) Task 3: In Progress item
- (?) Task 4: Question / Review item {priority: high}
- (!) Task 5: Important alert item

##[ 3. Callouts & Structured Elements ]

<callout>(type: tip)[
  Tip: You can customize rendering in React and Svelte using @tomet/react and @tomet/svelte!
]

<caution>[
  Be careful with duplicate IDs across elements.
]

##[ 4. Code Blocks ]

<codeblock>(lang: rust)[
use tomet_parser::parse_document;

fn main() {
    let doc = parse_document("#[ Hello Tomet ]").unwrap();
    println!("Parsed AST: {doc:#?}");
}
]

##[ 5. Macro Expansions ]

- Issue link: $gh(42)
- Greeting: $greet("Alice", "Bob")
- Footer: \${copyright}
`,
	tasks: `#[ Project Sprint Checklist ]

##[ Core Engine ]

- (x) Design AST Span Metadata {assignee: alice, priority: high}
- (x) Implement Lossless Formatter {status: done}
- (x) Add Java JNI Bindings (org.tomet.tomet) {status: done}
- (x) Implement @tomet/react and @tomet/svelte Packages {status: done}

##[ Tooling & Web ]

- (T) Build Web Playground {status: in_progress, tag: dev}
- (?) Review LSP Hover & Completion {status: pending}
- (!) Security audit {priority: urgent}
`,
	meta: `#[ Multi-Format Data Metadata ]

@meta(format: json){
  {
    "project": "Tomet",
    "active": true,
    "tags": ["markup", "rust", "parser"]
  }
}

@meta(format: yaml){
  database:
    host: 127.0.0.1
    port: 5432
    pool_size: 10
}

@meta(format: toml){
  [package]
  name = "tomet"
  version = "0.1.0"
  edition = "2024"
}
`,
	macros: `#[ Macro Templates & Interpolation ]

@config{
  macros: {
    gh: "https://github.com/tomet/tomet/issues/\${1}"
    gh_pr: "https://github.com/tomet/tomet/pull/\${1}"
    crate: "https://crates.io/crates/\${1}"
    badge: "<badge>(type: \${1})[\${2}]"
    author: "Tomet Development Team"
    year: "2026"
  }
}

Check issue $gh(101) or PR $gh_pr(5).
Powered by $crate("serde_tomet").

Author: \${author} (\${year})
`,
};
