const presets = {
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
`
};

document.addEventListener('DOMContentLoaded', () => {
  const editor = document.getElementById('editor');
  const presetSelect = document.getElementById('preset-select');
  const advancedCheck = document.getElementById('advanced-check');
  const statusText = document.getElementById('status-text');
  const statusDot = document.querySelector('.status-dot');

  const tabButtons = document.querySelectorAll('.tab');
  const tabViews = {
    preview: document.getElementById('tab-preview'),
    ast: document.getElementById('tab-ast'),
    markdown: document.getElementById('tab-markdown'),
    source: document.getElementById('tab-source'),
  };

  let activeTab = 'preview';

  tabButtons.forEach(btn => {
    btn.addEventListener('click', () => {
      tabButtons.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      activeTab = btn.dataset.tab;
      Object.keys(tabViews).forEach(k => {
        tabViews[k].style.display = (k === activeTab) ? 'block' : 'none';
      });
    });
  });

  presetSelect.addEventListener('change', (e) => {
    if (presets[e.target.value]) {
      editor.value = presets[e.target.value];
      triggerParse();
    }
  });

  advancedCheck.addEventListener('change', triggerParse);

  let debounceTimer;
  editor.addEventListener('input', () => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(triggerParse, 150);
  });

  async function triggerParse() {
    const source = editor.value;
    const advanced = advancedCheck.checked;
    try {
      const res = await fetch('/api/parse', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source, advanced })
      });
      const data = await res.json();
      if (data.ok) {
        statusDot.className = 'status-dot ok';
        statusText.textContent = 'Valid TypedMark Document';
        tabViews.preview.innerHTML = data.html;
        tabViews.ast.textContent = data.ast;
        tabViews.markdown.textContent = data.markdown;
        tabViews.source.textContent = data.html;
      } else {
        statusDot.className = 'status-dot err';
        statusText.textContent = `Parse Error line ${data.error.line}:${data.error.column} - ${data.error.message}`;
      }
    } catch (e) {
      statusDot.className = 'status-dot err';
      statusText.textContent = 'Server connection error';
    }
  }

  // Initialize
  editor.value = presets.cheatsheet;
  triggerParse();
});
