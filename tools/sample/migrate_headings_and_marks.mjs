#!/usr/bin/env node
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Parse command line arguments
const args = process.argv.slice(2);
let targetDir = '.';
let applyChanges = false;
let diffLimit = 10;
let excludePatterns = ['.git', 'node_modules', 'target'];

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
  } else if (args[i] === '--help' || args[i] === '-h') {
    console.log(`Usage: node migrate_headings_and_marks.mjs [options]
Options:
  --dir <path>        Target directory (default: .)
  --apply             Actually write changes to files (default is dry-run)
  --dry-run           Only report changes without writing files (default)
  --exclude <pats>    Comma-separated folder patterns to exclude (default: .git,node_modules,target)
  --limit <num>       Number of diff samples to display (default: 10)
  --help, -h          Show this help
`);
    process.exit(0);
  }
}

/**
 * Converts ==mark== to @mark[...] while strictly protecting inline code `...`
 */
function convertMarksInLine(line) {
  let count = 0;
  // Match either inline code `...` OR ==...==
  const converted = line.replace(/(`[^`\r\n]*`)|(?<!=)==(?!=)(.+?)(?<!=)==(?!=)/g, (match, code, markContent) => {
    if (code !== undefined) {
      // Inside inline code, keep it untouched
      return code;
    }
    count++;
    return `@mark[${markContent}]`;
  });
  return { line: converted, count };
}

/**
 * Converts # headings to = headings at line start
 */
function convertHeadingsInLine(line) {
  let count = 0;

  // Don't convert if it's a comment line
  const trimmed = line.trimStart();
  if (trimmed.startsWith('//') || trimmed.startsWith('/*')) {
    return { line, count };
  }

  // 1. Bracketed heading: #[ ... ] or ##[ ... ]
  // e.g. ##[ 経緯 ] -> ==[ 経緯 ]
  const bracketRegex = /^(\s*)(#+)(\[[^\r\n]*\])(.*)$/;
  const bracketMatch = line.match(bracketRegex);
  if (bracketMatch) {
    const indent = bracketMatch[1];
    const hashes = bracketMatch[2];
    const bracketContent = bracketMatch[3];
    const rest = bracketMatch[4];
    count++;
    return {
      line: `${indent}${'='.repeat(hashes.length)}${bracketContent}${rest}`,
      count,
    };
  }

  // 2. Space-separated sugar heading: # Heading or ## Heading
  // Must NOT match #(tag) sugar!
  const sugarRegex = /^(\s*)(#+)(\s+[^\r\n]+)$/;
  const sugarMatch = line.match(sugarRegex);
  if (sugarMatch) {
    const indent = sugarMatch[1];
    const hashes = sugarMatch[2];
    const content = sugarMatch[3];
    count++;
    return {
      line: `${indent}${'='.repeat(hashes.length)}${content}`,
      count,
    };
  }

  return { line, count };
}

/**
 * Transforms a file's content
 */
function transformContent(content) {
  const lines = content.split('\n');
  const newLines = [];
  let inFencedCode = false;
  let inPlusFence = false;
  let marksConverted = 0;
  let headingsConverted = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Check for ``` fenced code block
    if (trimmed.startsWith('```')) {
      inFencedCode = !inFencedCode;
      newLines.push(line);
      continue;
    }

    // Check for +++ raw fence
    if (trimmed === '+++' || trimmed.startsWith('+++') || trimmed.endsWith('+++')) {
      if (trimmed === '+++' || trimmed.length - trimmed.replace(/^\++/, '').length >= 3) {
        inPlusFence = !inPlusFence;
        newLines.push(line);
        continue;
      }
    }

    // Inside code fences, preserve verbatim
    if (inFencedCode || inPlusFence) {
      newLines.push(line);
      continue;
    }

    // Step 1: Convert ==mark== first
    const markRes = convertMarksInLine(line);
    marksConverted += markRes.count;

    // Step 2: Convert # headings to = headings second
    const headingRes = convertHeadingsInLine(markRes.line);
    headingsConverted += headingRes.count;

    newLines.push(headingRes.line);
  }

  return {
    content: newLines.join('\n'),
    marksConverted,
    headingsConverted,
    hasChanged: marksConverted > 0 || headingsConverted > 0,
  };
}

/**
 * Recursively find all .tmt files
 */
async function collectTmtFiles(dir, fileList = []) {
  try {
    const entries = await fs.readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (excludePatterns.some((p) => entry.name === p || entry.name.includes(p))) {
        continue;
      }
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await collectTmtFiles(fullPath, fileList);
      } else if (entry.isFile() && entry.name.endsWith('.tmt')) {
        fileList.push(fullPath);
      }
    }
  } catch (err) {
    console.error(`Error reading directory ${dir}: ${err.message}`);
  }
  return fileList;
}

/**
 * Generate a unified diff preview between two strings
 */
function createDiffPreview(oldContent, newContent, filePath, maxLines = 15) {
  const oldLines = oldContent.split('\n');
  const newLines = newContent.split('\n');
  const diffs = [];

  for (let i = 0; i < Math.max(oldLines.length, newLines.length); i++) {
    if (oldLines[i] !== newLines[i]) {
      if (oldLines[i] !== undefined) {
        diffs.push(`- L${i + 1}: ${oldLines[i]}`);
      }
      if (newLines[i] !== undefined) {
        diffs.push(`+ L${i + 1}: ${newLines[i]}`);
      }
      if (diffs.length >= maxLines) {
        diffs.push('  ...');
        break;
      }
    }
  }

  return diffs.join('\n');
}

async function main() {
  console.log(`\n========================================================`);
  console.log(` Tomet Migration: # -> = Headings & ==mark== -> @mark[]`);
  console.log(` Mode: ${applyChanges ? '🚀 APPLY (writing to files)' : '🔍 DRY-RUN (read-only preview)'}`);
  console.log(` Target Directory: ${targetDir}`);
  console.log(`========================================================\n`);

  const startTime = Date.now();

  console.log(`Scanning for .tmt files...`);
  const files = await collectTmtFiles(targetDir);
  console.log(`Found ${files.length.toLocaleString()} .tmt files in ${(Date.now() - startTime) / 1000}s.\n`);

  let modifiedFilesCount = 0;
  let totalMarks = 0;
  let totalHeadings = 0;
  const sampleDiffs = [];

  const batchSize = 100;
  for (let i = 0; i < files.length; i += batchSize) {
    const batch = files.slice(i, i + batchSize);
    await Promise.all(
      batch.map(async (file) => {
        try {
          const original = await fs.readFile(file, 'utf8');
          const res = transformContent(original);

          if (res.hasChanged) {
            modifiedFilesCount++;
            totalMarks += res.marksConverted;
            totalHeadings += res.headingsConverted;

            if (sampleDiffs.length < diffLimit) {
              const diff = createDiffPreview(original, res.content, file);
              sampleDiffs.push({ file, diff });
            }

            if (applyChanges) {
              await fs.writeFile(file, res.content, 'utf8');
            }
          }
        } catch (err) {
          console.error(`Error processing ${file}: ${err.message}`);
        }
      })
    );

    if ((i + batchSize) % 5000 === 0 || i + batchSize >= files.length) {
      const processed = Math.min(i + batchSize, files.length);
      const pct = ((processed / files.length) * 100).toFixed(1);
      process.stdout.write(`Processed ${processed.toLocaleString()} / ${files.length.toLocaleString()} (${pct}%)...\r`);
    }
  }

  console.log(`\n\n----------------- SUMMARY -----------------`);
  console.log(`Total .tmt files scanned : ${files.length.toLocaleString()}`);
  console.log(`Files with changes       : ${modifiedFilesCount.toLocaleString()}`);
  console.log(`==mark== converted       : ${totalMarks.toLocaleString()}`);
  console.log(`# headings converted     : ${totalHeadings.toLocaleString()}`);
  console.log(`Elapsed time             : ${((Date.now() - startTime) / 1000).toFixed(2)}s`);
  console.log(`-------------------------------------------\n`);

  if (sampleDiffs.length > 0) {
    console.log(`=== Sample Diffs (${sampleDiffs.length} shown) ===\n`);
    for (const sample of sampleDiffs) {
      console.log(`File: ${sample.file}`);
      console.log(sample.diff);
      console.log('');
    }
  }

  if (!applyChanges && modifiedFilesCount > 0) {
    console.log(`💡 To apply these changes, re-run with:`);
    console.log(`   node migrate_headings_and_marks.mjs --apply\n`);
  }
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
