#!/usr/bin/env node
/** Execute OmniRoute's pinned pure RTK -> Caveman modules without starting its gateway. */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";


function parseArgs(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error(`invalid argument sequence near ${key ?? "<end>"}`);
    }
    values.set(key.slice(2), value);
  }
  return values;
}


function visibleText(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return JSON.stringify(content ?? "");
  return content
    .map((item) => {
      if (typeof item === "string") return item;
      if (item && typeof item === "object" && typeof item.text === "string") return item.text;
      return "";
    })
    .filter(Boolean)
    .join("\n");
}


const args = parseArgs(process.argv.slice(2));
const repository = path.resolve(args.get("repo") ?? "");
if (!fs.existsSync(path.join(repository, "open-sse", "services", "compression"))) {
  throw new Error(`invalid OmniRoute repository: ${repository}`);
}

const rtkUrl = pathToFileURL(
  path.join(repository, "open-sse", "services", "compression", "engines", "rtk", "index.ts")
).href;
const cavemanUrl = pathToFileURL(
  path.join(repository, "open-sse", "services", "compression", "caveman.ts")
).href;
const [{ applyRtkCompression }, { cavemanCompress }] = await Promise.all([
  import(rtkUrl),
  import(cavemanUrl),
]);

const input = JSON.parse(fs.readFileSync(0, "utf8"));
const results = [];
for (const benchmarkCase of input.cases ?? []) {
  const message =
    benchmarkCase.surface === "tool_output"
      ? {
          role: "tool",
          tool_call_id: `benchmark-${benchmarkCase.id}`,
          content: benchmarkCase.text,
        }
      : { role: "user", content: benchmarkCase.text };
  const original = { messages: [message] };
  const rtk = applyRtkCompression(original, { stepConfig: { intensity: "standard" } });
  const caveman = cavemanCompress(rtk.body, {
    enabled: true,
    compressRoles: ["user"],
    intensity: "full",
  });
  const returnedMessage = caveman.body?.messages?.[0] ?? rtk.body?.messages?.[0] ?? message;
  results.push({
    case_id: benchmarkCase.id,
    primary_output: visibleText(returnedMessage.content),
    native_metrics: {
      pipeline: ["rtk:standard", "caveman:full"],
      rtk_compressed: Boolean(rtk.compressed),
      rtk_stats: rtk.stats ?? null,
      caveman_compressed: Boolean(caveman.compressed),
      caveman_stats: caveman.stats ?? null,
    },
  });
}

process.stdout.write(`${JSON.stringify({ results })}\n`);
