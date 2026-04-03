import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..');
const planPath = path.join(repoRoot, '.codex', 'deku-build-plan.md');
const outputPath = path.join(
  repoRoot,
  'docs',
  'src',
  'content',
  'docs',
  'reference',
  'cli-reference.md',
);

const plan = await readFile(planPath, 'utf8');
const commandSection = extractSection(
  plan,
  '#### [x] 4.1 CLI Binary (`crates/deku/src/`)',
  '#### [x] 4.3 Plugin Loader',
);
const summarySection = extractSection(plan, '## 16. Command Reference', '## 17.');

const commandBlocks = parseCommandBlocks(commandSection);
const summaryCommands = parseSummaryCommands(summarySection);

const markdown = `---
title: CLI Reference
description: Generated command reference for the current Deku CLI surface.
---

This page is generated from [\`.codex/deku-build-plan.md\`](../../../../.codex/deku-build-plan.md).
It reflects the current planned and implemented CLI surface in the repository, not a polished release contract.

## Implemented Command Summary

${summaryCommands.map((command) => `- \`${command}\``).join('\n')}

## Command Groups

${commandBlocks
  .map(
    ({ command, bullets }) => `### \`${command}\`

${bullets.map((bullet) => `- ${bullet}`).join('\n')}`,
  )
  .join('\n\n')}
`;

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, markdown);

function extractSection(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  if (start === -1) return '';
  const end = source.indexOf(endMarker, start);
  return source.slice(start, end === -1 ? undefined : end);
}

function parseSummaryCommands(section) {
  return section
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.startsWith('- `deku '))
    .map((line) => line.replace(/^- `|`$/g, ''));
}

function parseCommandBlocks(section) {
  const lines = section.split('\n');
  const blocks = [];
  let current = null;

  for (const rawLine of lines) {
    const line = rawLine.trim();
    const header = line.match(/^\*\*`([^`]+)`\*\*/);

    if (header) {
      if (current) blocks.push(current);
      current = header[1].startsWith('deku ')
        ? { command: header[1], bullets: [] }
        : null;
      continue;
    }

    if (!current) continue;
    if (line.startsWith('- ')) {
      current.bullets.push(line.slice(2));
    }
  }

  if (current) blocks.push(current);
  return blocks;
}
