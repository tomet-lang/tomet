export const presets: Record<string, string> = {
	cheatsheet: `#[ TypedMark Quick Overview ]

<caution>[ TypedMark (TM) combines standard Markdown prose with typed structure. ]

##[ Features ]

- ( ) Task 1: Basic checkbox
- (x) Task 2: Completed task
- (T) Task 3: In Progress status
- (?) Task 4: Question status {tag: urgent}

<codeblock>(lang:rust)[
fn main() {
    println!("Hello TypedMark!");
}
]

---[ Embedded Data Example ]---

@meta(format:json){
  {
    "author": "Antigravity",
    "version": "1.0"
  }
}
`,
	tasks: `#[ Project Sprint Checklist ]

- (x) Design AST Span Metadata {assignee: alice}
- (x) Implement Lossless Formatter {status: done}
- (T) Build Web Playground {status: in_progress, tag: dev}
- (?) Review LSP Hover & Completion {status: pending}
- (!) Security audit {priority: high}
`,
	meta: `#[ Multi-Format Data Metadata ]

@meta(format:json){
  {
    "project": "TypedMark",
    "active": true,
    "tags": ["markup", "rust", "parser"]
  }
}

@meta(format:yaml){
  database:
    host: 127.0.0.1
    port: 5432
}

@meta(format:toml){
  [package]
  name = "typedmark"
  version = "1.0.0"
}
`,
};
