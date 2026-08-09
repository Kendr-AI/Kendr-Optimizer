# Remove KendrOptimizer from NanoClaw

Run the inverse guarded patch, then remove the two skill-owned source files:

```bash
node .claude/skills/add-kendr-optimizer/assets/apply-patch.mjs --remove .
rm container/agent-runner/src/kendr-optimizer.ts
rm container/agent-runner/src/kendr-optimizer.test.ts
pnpm exec tsc -p container/agent-runner/tsconfig.json --noEmit
./container/build.sh
```

Update existing group overlays that contain copies of these files and restart
those groups. Removing this source adapter does not stop or delete a separately
deployed Kendr sidecar; that lifecycle remains operator-owned.

