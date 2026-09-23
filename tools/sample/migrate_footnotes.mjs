#!/usr/bin/env node
import fs from 'node:fs/promises';
import path from 'node:path';

// Parse command line arguments
const args = process.argv.slice(2);
let targetDir = '.';
let applyChanges = false;
let diffLimit = 15;
let excludePatterns = ['.git', 'node_modules', 'target'];
let showReport = true;

for (let i = 0; i < args.length; i++) {
  if (args[i] === '--apply') {
    applyChanges = true;
  } else if (args[i] === '--dry-run') {
    applyChanges = false;
  } else if (args[i] === '--dir' && i + 1 < args.length) {
    targetDir = args[++i];
  } else if (args[i] === '--exclude' && i + 1 < args.length) {
    excludePatterns.push(...args[++i].split(',').map((s) => s.trim()));
  } else if (args[i] === '--limit' && i + 1 < args.length) {
    diffLimit = parseInt(args[++i], 10);
  } else if (args[i] === '--no-report') {
    showReport = false;
  } else if (args[i] === '--help' || args[i] === '-h') {
    console.log(`Usage: node migrate_footnotes.mjs [options]
Options:
  --dir <path>        Target directory (default: .)
  --apply             Actually write changes to files (default is dry-run)
  --dry-run           Only report changes without writing files (default)
  --exclude <pats>    Comma-separated folder patterns to exclude (default: .git,node_modules,target)
  --limit <num>       Number of diff samples to display (default: 15)
  --no-report         Suppress dangling/unused footnote report
  --help, -h          Show this help
`);
    process.exit(0);
  }
}

/**
 * Recursively find all .tmt files
 */
async function findTmtFiles(dir, excludes) {
  const dirents = await fs.readdir(dir, { withFileTypes: true });
  const files = [];

  for (const dirent of dirents) {
    const fullPath = path.resolve(dir, dirent.name);
    const relName = dirent.name;

    if (excludes.some((pat) => relName === pat || fullPath.includes(`/${pat}/`))) {
      continue;
    }

    if (dirent.isDirectory()) {
      const subFiles = await findTmtFiles(fullPath, excludes);
      files.push(...subFiles);
    } else if (dirent.isFile() && relName.endsWith('.tmt')) {
      files.push(fullPath);
    }
  }

  return files;
}

/**
 * Migrates broken footnotes in a document
 */
function migrateFootnotes(content) {
  const lines = content.split('\n');
  let inFence = false;
  let defCount = 0;
  let linkMalCount = 0;
  let refCount = 0;
  let modified = false;

  const newLines = [];
  const danglingRefs = [];
  const unusedDefs = [];

  // Pass 1: Line by line transformation
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Check code fences (``` or +++)
    if (trimmed.startsWith('```') || trimmed.startsWith('+++')) {
      inFence = !inFence;
      newLines.push(line);
      continue;
    }

    if (inFence) {
      newLines.push(line);
      continue;
    }

    // 1. Definition lines: ^(\s*)\[`\^`([0-9]*)\]:\s*(.*)$
    const defMatch = line.match(/^(\s*)\[\`\^\`([0-9]*)\]:\s*(.*)$/);
    if (defMatch) {
      const indent = defMatch[1];
      const id = defMatch[2];
      const rest = defMatch[3];
      defCount++;
      modified = true;
      if (id) {
        newLines.push(`${indent}@footnote(${id})[${rest}]`);
      } else {
        newLines.push(`${indent}@footnote[${rest}]`);
      }
      continue;
    }

    let currentLine = line;

    // 2. Link malformation: @link("[text](url)")[^`id]
    const malMatches = currentLine.match(/@link\("\[(.*?)\]\((.*?)\)"\)\[\`\^\`([0-9]*)\]/g);
    if (malMatches) {
      linkMalCount += malMatches.length;
      modified = true;
      currentLine = currentLine.replace(
        /@link\("\[(.*?)\]\((.*?)\)"\)\[\`\^\`([0-9]*)\]/g,
        (_, text, url, id) => {
          return `@link("${url}")[${text}]^(${id})`;
        }
      );
    }

    // 3. Normal refs: [`^`id] where id is [0-9]*
    const refMatches = currentLine.match(/\[\`\^\`([0-9]*)\]/g);
    if (refMatches) {
      refCount += refMatches.length;
      modified = true;
      currentLine = currentLine.replace(/\[\`\^\`([0-9]*)\]/g, (_, id) => {
        return `^(${id})`;
      });
    }

    newLines.push(currentLine);
  }

  const newContent = newLines.join('\n');

  // Pass 2: Check semantic consistency in migrated content
  const definedIds = new Set();
  const referencedIds = new Set();

  for (const line of newLines) {
    // Collect definitions
    const defMatches = line.matchAll(/@footnote(?:\(([0-9]+)\))?\[/g);
    for (const m of defMatches) {
      if (m[1]) definedIds.add(m[1]);
    }
    // Collect references
    const refMatches = line.matchAll(/\^(?:\(([0-9]+)\))/g);
    for (const m of refMatches) {
      if (m[1]) referencedIds.add(m[1]);
    }
  }

  for (const id of referencedIds) {
    if (!definedIds.has(id)) {
      danglingRefs.push(id);
    }
  }
  for (const id of definedIds) {
    if (!referencedIds.has(id)) {
      unusedDefs.push(id);
    }
  }

  return {
    content: newContent,
    modified,
    counts: {
      defCount,
      linkMalCount,
      refCount,
      total: defCount + linkMalCount + refCount,
    },
    consistency: {
      danglingRefs,
      unusedDefs,
    },
  };
}

async function main() {
  const resolvedTarget = path.resolve(targetDir);
  console.log(`Target directory: ${resolvedTarget}`);
  console.log(`Mode: ${applyChanges ? 'APPLY (writing files)' : 'DRY-RUN (read-only)'}`);
  console.log('Scanning files...');

  const startTime = Date.now();
  const files = await findTmtFiles(resolvedTarget, excludePatterns);
  const scanTime = Date.now() - startTime;
  console.log(`Found ${files.length} .tmt files in ${scanTime}ms.`);

  let modifiedFiles = 0;
  let totalDef = 0;
  let totalLinkMal = 0;
  let totalRef = 0;

  const diffSamples = [];
  const danglingReport = [];
  const unusedReport = [];

  for (const file of files) {
    const raw = await fs.readFile(file, 'utf-8');
    if (!raw.includes('[`^`')) continue;

    const result = migrateFootnotes(raw);
    if (!result.modified) continue;

    modifiedFiles++;
    totalDef += result.counts.defCount;
    totalLinkMal += result.counts.linkMalCount;
    totalRef += result.counts.refCount;

    const relPath = path.relative(resolvedTarget, file);

    if (result.consistency.danglingRefs.length > 0) {
      danglingReport.push({
        file: relPath,
        missingDefs: result.consistency.danglingRefs,
      });
    }
    if (result.consistency.unusedDefs.length > 0) {
      unusedReport.push({
        file: relPath,
        unusedDefs: result.consistency.unusedDefs,
      });
    }

    if (diffSamples.length < diffLimit) {
      // Find modified lines for display
      const oldLines = raw.split('\n');
      const newLines = result.content.split('\n');
      const diffs = [];

      for (let i = 0; i < Math.max(oldLines.length, newLines.length); i++) {
        if (oldLines[i] !== newLines[i]) {
          diffs.push({ line: i + 1, old: oldLines[i], new: newLines[i] });
        }
      }

      diffSamples.push({ file: relPath, diffs });
    }

    if (applyChanges) {
      await fs.writeFile(file, result.content, 'utf-8');
    }
  }

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(2);
  console.log('\n================ Migration Summary ================');
  console.log(`Scanned files:       ${files.length}`);
  console.log(`Modified files:      ${modifiedFiles}`);
  console.log(`Footnote definitions: ${totalDef} converted to @footnote(id)[...]`);
  console.log(`Link malformations:  ${totalLinkMal} fixed to @link("url")[title]^(id)`);
  console.log(`Prose references:    ${totalRef} converted to ^(id)`);
  console.log(`Total occurrences:   ${totalDef + totalLinkMal + totalRef}`);
  console.log(`Elapsed time:        ${elapsed}s`);
  console.log('===================================================\n');

  if (diffSamples.length > 0) {
    console.log(`--- Sample Diffs (first ${diffSamples.length}) ---`);
    for (const sample of diffSamples) {
      console.log(`\nFile: ${sample.file}`);
      for (const d of sample.diffs.slice(0, 5)) {
        console.log(`  Line ${d.line}:`);
        console.log(`    - ${d.old}`);
        console.log(`    + ${d.new}`);
      }
      if (sample.diffs.length > 5) {
        console.log(`    ... and ${sample.diffs.length - 5} more lines`);
      }
    }
  }

  if (showReport) {
    console.log('\n================ Semantic Consistency Report ================');
    console.log(`Files with dangling references (missing definitions): ${danglingReport.length}`);
    console.log(`Files with unused definitions (unreferenced in prose):  ${unusedReport.length}`);

    if (danglingReport.length > 0) {
      console.log('\n[Dangling References Sample (first 10)]');
      for (const item of danglingReport.slice(0, 10)) {
        console.log(`  - ${item.file} (missing definitions for IDs: ${item.missingDefs.join(', ')})`);
      }
      if (danglingReport.length > 10) {
        console.log(`    ... and ${danglingReport.length - 10} more files`);
      }
    }

    if (unusedReport.length > 0) {
      console.log('\n[Unused Definitions Sample (first 10)]');
      for (const item of unusedReport.slice(0, 10)) {
        console.log(`  - ${item.file} (unused definitions for IDs: ${item.unusedDefs.join(', ')})`);
      }
      if (unusedReport.length > 10) {
        console.log(`    ... and ${unusedReport.length - 10} more files`);
      }
    }
    console.log('==============================================================');

    // Write full audit report to file
    const reportPath = path.resolve('tools/sample/footnote_audit_report.md');
    let md = `# Footnote Audit Report\n\n`;
    md += `Generated at: ${new Date().toISOString()}\n`;
    md += `Target: \`${resolvedTarget}\`\n\n`;
    md += `## 1. Dangling References (${danglingReport.length} files)\n`;
    md += `These files contain footnote references in text, but no corresponding \`@footnote(id)\` definition was found.\n\n`;
    for (const item of danglingReport) {
      md += `- \`${item.file}\` (missing: \`${item.missingDefs.join(', ')}\`)\n`;
    }
    md += `\n## 2. Unused Definitions (${unusedReport.length} files)\n`;
    md += `These files contain \`@footnote(id)\` definitions, but no corresponding reference was found in text.\n\n`;
    for (const item of unusedReport) {
      md += `- \`${item.file}\` (unused: \`${item.unusedDefs.join(', ')}\`)\n`;
    }
    await fs.writeFile(reportPath, md, 'utf-8');
    console.log(`\nSaved full audit report to: ${reportPath}`);
  }

  if (!applyChanges) {
    console.log('\n[DRY RUN COMPLETE] No files were modified. Run with --apply to execute.');
  } else {
    console.log('\n[APPLY COMPLETE] All changes written successfully.');
  }
}

main().catch((err) => {
  console.error('Fatal error during migration:', err);
  process.exit(1);
});
