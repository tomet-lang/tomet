#!/usr/bin/env -S deno run --allow-read

import { walk } from "jsr:@std/fs/walk";

const KANA_REGEX = /[\u3040-\u30ff]/;
const SKIP_DIRS = ["target", ".git", ".agents", "node_modules"];

let failed = false;

for await (
  const entry of walk(".", {
    match: [/\.writ\.tmt$/],
    skip: SKIP_DIRS.map((d) => new RegExp(`(^|/)${d}(/|$)`)),
    includeDirs: false,
  })
) {
  const content = await Deno.readTextFile(entry.path);
  const lines = content.split(/\r?\n/);

  for (let i = 0; i < lines.length; i++) {
    if (KANA_REGEX.test(lines[i])) {
      console.error(
        `LANGUAGE FAIL: ${entry.path}:${i + 1} -- kana in a writ; docs-language says every .writ.tmt is English`,
      );
      failed = true;
    }
  }
}

if (failed) {
  Deno.exit(1);
}
