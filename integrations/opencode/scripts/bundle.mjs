import { build } from "esbuild"

await build({
  entryPoints: ["src/plugin.ts"],
  bundle: true,
  format: "esm",
  platform: "node",
  target: "node22",
  outfile: "dist/kendr-optimizer.js",
  banner: {
    js: "// Installed by Kendr Optimizer. This bundle exports one OpenCode plugin factory.",
  },
})
