#!/usr/bin/env node
import { createClaudeCodeBridgeServer, DEFAULT_BRIDGE_HOST, DEFAULT_BRIDGE_PORT, DEFAULT_CORE_ENDPOINT, validateNumericLoopbackEndpoint, } from "./index.js";
function envBoolean(value) {
    return value === "1" || value?.toLowerCase() === "true";
}
function envPort(value) {
    if (!value)
        return DEFAULT_BRIDGE_PORT;
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > 65_535) {
        throw new Error("KENDR_CLAUDE_BRIDGE_PORT must be an integer from 1 to 65535");
    }
    return parsed;
}
const endpoint = process.env.KENDR_OPTIMIZER_ENDPOINT ?? DEFAULT_CORE_ENDPOINT;
validateNumericLoopbackEndpoint(endpoint);
const port = envPort(process.env.KENDR_CLAUDE_BRIDGE_PORT);
const server = createClaudeCodeBridgeServer({
    coreEndpoint: endpoint,
    shadow: envBoolean(process.env.KENDR_OPTIMIZER_SHADOW),
});
server.listen(port, DEFAULT_BRIDGE_HOST, () => {
    process.stdout.write("Kendr Claude Code bridge listening on http://" +
        DEFAULT_BRIDGE_HOST +
        ":" +
        String(port) +
        "; optimizer=" +
        endpoint +
        "; provider-egress=false\n");
});
for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => {
        server.close(() => process.exit(0));
    });
}
//# sourceMappingURL=server.js.map