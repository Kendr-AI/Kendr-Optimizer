#!/usr/bin/env node

import { readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const args = new Set(process.argv.slice(2));
const remove = args.has('--remove');
const check = args.has('--check');
const checkSource = args.has('--check-source');
const rootArg = process.argv.slice(2).find((arg) => !arg.startsWith('--')) ?? '.';
const root = resolve(rootArg);
const path = resolve(root, 'container/agent-runner/src/poll-loop.ts');

const IMPORT_ANCHOR =
  "import type { AgentProvider, AgentQuery, ProviderEvent, ProviderExchange } from './providers/types.js';";
const IMPORT = "import { optimizeNanoClawPrompt } from './kendr-optimizer.js';";
const INITIAL_BEFORE = `const query = config.provider.query({
      prompt,
      continuation,`;
const INITIAL_AFTER = `const query = config.provider.query({
      prompt: await optimizeNanoClawPrompt(prompt),
      continuation,`;
const FOLLOWUP_BEFORE = `query.push(prompt);
        archivePrompts.push(prompt);`;
const FOLLOWUP_AFTER = `const optimizedPrompt = await optimizeNanoClawPrompt(prompt);
        if (done) return;
        query.push(optimizedPrompt);
        archivePrompts.push(prompt);`;

const onDisk = readFileSync(path, 'utf8');
const eol = onDisk.includes('\r\n') ? '\r\n' : '\n';
let source = onDisk.replaceAll('\r\n', '\n');

if (checkSource) {
  const unpatched = count(source, INITIAL_BEFORE) === 1 && count(source, FOLLOWUP_BEFORE) === 1;
  const patched = count(source, INITIAL_AFTER) === 1 && count(source, FOLLOWUP_AFTER) === 1;
  if (unpatched === patched) {
    throw new Error('prompt seams are mixed or ambiguous; refusing this NanoClaw source');
  }
  assertCount(source, IMPORT_ANCHOR, 1, 'provider-types import anchor');
  assertCount(source, IMPORT, patched ? 1 : 0, 'Kendr import');
  process.stdout.write(`NanoClaw source has one coherent ${patched ? 'patched' : 'unpatched'} prompt seam.\n`);
  process.exit(0);
}

if (check) {
  assertCount(source, IMPORT, 1, 'Kendr import');
  assertCount(source, INITIAL_AFTER, 1, 'optimized initial provider.query seam');
  assertCount(source, FOLLOWUP_AFTER, 1, 'optimized query.push seam');
  assertCount(source, INITIAL_BEFORE, 0, 'unpatched initial provider.query seam');
  assertCount(source, FOLLOWUP_BEFORE, 0, 'unpatched query.push seam');
  process.stdout.write('KendrOptimizer NanoClaw patch is present.\n');
  process.exit(0);
}

if (remove) {
  source = replaceExactly(source, `${IMPORT}\n`, '', 'Kendr import');
  source = replaceExactly(source, INITIAL_AFTER, INITIAL_BEFORE, 'initial provider.query seam');
  source = replaceExactly(source, FOLLOWUP_AFTER, FOLLOWUP_BEFORE, 'query.push seam');
} else {
  if (!source.includes(IMPORT)) {
    source = replaceExactly(
      source,
      `${IMPORT_ANCHOR}\n`,
      `${IMPORT_ANCHOR}\n${IMPORT}\n`,
      'provider-types import anchor',
    );
  } else {
    assertCount(source, IMPORT, 1, 'Kendr import');
  }
  source = installSeam(source, INITIAL_BEFORE, INITIAL_AFTER, 'initial provider.query seam');
  source = installSeam(source, FOLLOWUP_BEFORE, FOLLOWUP_AFTER, 'query.push seam');
}

const output = eol === '\r\n' ? source.replaceAll('\n', '\r\n') : source;
if (output !== onDisk) {
  const temporary = `${path}.kendr-${process.pid}.tmp`;
  writeFileSync(temporary, output, 'utf8');
  renameSync(temporary, path);
}
process.stdout.write(remove ? 'KendrOptimizer NanoClaw patch removed.\n' : 'KendrOptimizer NanoClaw patch installed.\n');

function installSeam(value, before, after, label) {
  const beforeCount = count(value, before);
  const afterCount = count(value, after);
  if (beforeCount === 1 && afterCount === 0) return value.replace(before, after);
  if (beforeCount === 0 && afterCount === 1) return value;
  throw new Error(
    `${label} drifted (unpatched=${beforeCount}, patched=${afterCount}); refusing an ambiguous edit`,
  );
}

function replaceExactly(value, before, after, label) {
  assertCount(value, before, 1, label);
  return value.replace(before, after);
}

function assertCount(value, needle, expected, label) {
  const actual = count(value, needle);
  if (actual !== expected) {
    throw new Error(`${label}: expected ${expected} occurrence(s), found ${actual}`);
  }
}

function count(value, needle) {
  return value.split(needle).length - 1;
}
