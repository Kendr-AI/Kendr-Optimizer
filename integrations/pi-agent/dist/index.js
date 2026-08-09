import { randomUUID } from "node:crypto";
export const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export const PI_AUDIT = Object.freeze({
    package: "@earendil-works/pi-coding-agent",
    version: "0.84.1",
    commit: "53fa77ccd8a279eb87e92294ef3687b03ff80112",
});
export class LoopbackEndpointError extends Error {
    constructor(message) {
        super(message);
        this.name = "LoopbackEndpointError";
    }
}
export function validateNumericLoopbackEndpoint(raw) {
    let endpoint;
    try {
        endpoint = new URL(raw);
    }
    catch {
        throw new LoopbackEndpointError("Kendr endpoint must be an absolute URL");
    }
    const hostname = endpoint.hostname.toLowerCase();
    if (endpoint.protocol !== "http:") {
        throw new LoopbackEndpointError("Kendr endpoint must use HTTP on loopback");
    }
    if (hostname !== "127.0.0.1" && hostname !== "[::1]" && hostname !== "::1") {
        throw new LoopbackEndpointError("Kendr endpoint must use numeric loopback");
    }
    if (endpoint.username || endpoint.password) {
        throw new LoopbackEndpointError("Kendr endpoint must not contain credentials");
    }
    if (endpoint.pathname !== "/" || endpoint.search || endpoint.hash) {
        throw new LoopbackEndpointError("Kendr endpoint must not contain a path, query, or fragment");
    }
    return endpoint;
}
export class KendrPiAdapter {
    #endpoint;
    #timeoutMs;
    #shadow;
    #fetch;
    constructor(options = {}) {
        this.#endpoint = validateNumericLoopbackEndpoint(options.coreEndpoint ?? DEFAULT_CORE_ENDPOINT);
        this.#timeoutMs = boundedPositiveInteger(options.timeoutMs, 100, 10_000);
        this.#shadow = options.shadow === true;
        this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
    }
    async optimizeSystemPrompt(event, context) {
        if (event.systemPrompt.length === 0)
            return undefined;
        const requestId = "pi-system-" + randomUUID();
        const messageId = "pi-system-message";
        const request = this.#buildRequest("request", requestId, [
            {
                id: messageId,
                role: "system",
                parts: [{ type: "text", text: event.systemPrompt }],
                metadata: { host: "pi", seam: "before_agent_start" },
            },
        ], context);
        const raw = await this.#dispatch(request);
        if (this.#shadow)
            return undefined;
        return decodeSingleText(raw, requestId, messageId, "system", event.systemPrompt);
    }
    async optimizeContext(event, context) {
        const encoded = encodeContext(event.messages);
        if (encoded.bindings.length === 0)
            return undefined;
        const requestId = "pi-context-" + randomUUID();
        const request = this.#buildRequest("history_ingest", requestId, encoded.messages, context);
        const raw = await this.#dispatch(request);
        if (this.#shadow)
            return undefined;
        const replacements = decodeContextOutcome(raw, requestId, encoded);
        if (!replacements)
            return undefined;
        return applyContextBindings(event.messages, encoded.bindings, replacements);
    }
    async optimizeToolResult(event, context) {
        const textBlocks = [];
        for (let index = 0; index < event.content.length; index += 1) {
            const block = event.content[index];
            if (!block)
                continue;
            if (block.type === "text" && block.text.length > 0) {
                textBlocks.push({
                    index,
                    text: block.text,
                    callId: event.toolCallId + ":" + textBlocks.length,
                });
            }
        }
        if (textBlocks.length === 0)
            return undefined;
        const requestId = "pi-tool-result-" + randomUUID();
        const messageId = "pi-tool-result-message";
        const parts = textBlocks.map((block) => ({
            type: "tool_result",
            call_id: block.callId,
            name: event.toolName,
            content: block.text,
            is_error: event.isError,
        }));
        const request = this.#buildRequest("tool_result", requestId, [
            {
                id: messageId,
                role: "tool",
                parts,
                metadata: { host: "pi", seam: "tool_result" },
            },
        ], context);
        const raw = await this.#dispatch(request);
        if (this.#shadow)
            return undefined;
        const replacements = decodeToolParts(raw, requestId, messageId, textBlocks);
        if (!replacements)
            return undefined;
        const copy = structuredClone(event.content);
        let changed = false;
        for (let index = 0; index < textBlocks.length; index += 1) {
            const binding = textBlocks[index];
            const replacement = replacements[index];
            if (!binding || replacement === undefined)
                return undefined;
            const block = copy[binding.index];
            if (!block || block.type !== "text")
                return undefined;
            if (block.text !== replacement)
                changed = true;
            block.text = replacement;
        }
        return changed ? copy : undefined;
    }
    async observeAssistantOutput(event, context) {
        const message = event.message;
        if (!isRecord(message) || message.role !== "assistant")
            return;
        const encoded = encodeContext([message]);
        if (encoded.bindings.length === 0)
            return;
        const requestId = "pi-output-observation-" + randomUUID();
        const request = this.#buildRequest("output_observation", requestId, encoded.messages, context, true);
        await this.#post("/v1/analyze", request);
    }
    #buildRequest(phase, requestId, messages, context, forceShadow = false) {
        const target = { tokenizer_profile: "approximate" };
        const model = context.model;
        if (model?.id)
            target.model = model.id;
        const contextWindow = model?.contextWindow;
        if (Number.isSafeInteger(contextWindow) && contextWindow !== undefined && contextWindow > 0) {
            target.context_limit = contextWindow;
        }
        const sessionId = safeSessionId(context);
        return {
            schema_version: "kendr.optimize/v1",
            phase,
            request_id: requestId,
            ...(sessionId ? { session_id: sessionId } : {}),
            content: {
                messages,
                tools: [],
                metadata: {
                    adapter: "@kendr/optimizer-pi",
                    host_version: PI_AUDIT.version,
                    provider_egress: false,
                },
            },
            target,
            host_capabilities: {
                can_narrow_tools: false,
                can_restore_references: false,
                can_retry_with_full_tools: false,
                streaming_output: true,
                can_set_max_output_tokens: false,
                can_set_verbosity: false,
                can_append_generation_policy: false,
            },
            policy: {
                risk_ceiling: "representation_safe",
                min_gain_tokens: 1,
                min_gain_percent: 0,
                latency_budget_ms: 100,
                preserve_cache_prefix: true,
                shadow: forceShadow || this.#shadow,
                enable_tool_selection: false,
                enable_lossy_tool_output: false,
                enable_generation_policy: false,
            },
        };
    }
    async #dispatch(request) {
        return this.#post(this.#shadow ? "/v1/analyze" : "/v1/optimize", request);
    }
    async #post(path, request) {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), this.#timeoutMs);
        try {
            const response = await this.#fetch(new URL(path, this.#endpoint), {
                method: "POST",
                headers: {
                    "content-type": "application/json",
                    accept: "application/json",
                },
                body: JSON.stringify(request),
                signal: controller.signal,
            });
            if (!response.ok)
                return undefined;
            return await response.json();
        }
        catch {
            return undefined;
        }
        finally {
            clearTimeout(timer);
        }
    }
}
export function createPiOptimizerExtension(options = {}) {
    const adapter = new KendrPiAdapter(options);
    return (pi) => {
        pi.on("before_agent_start", async (event, context) => {
            try {
                const systemPrompt = await adapter.optimizeSystemPrompt(event, context);
                return systemPrompt === undefined ? undefined : { systemPrompt };
            }
            catch {
                return undefined;
            }
        });
        pi.on("context", async (event, context) => {
            try {
                const messages = await adapter.optimizeContext(event, context);
                return messages === undefined ? undefined : { messages };
            }
            catch {
                return undefined;
            }
        });
        pi.on("tool_result", async (event, context) => {
            try {
                const content = await adapter.optimizeToolResult(event, context);
                return content === undefined ? undefined : { content };
            }
            catch {
                return undefined;
            }
        });
        if (options.observeOutput === true) {
            pi.on("message_end", async (event, context) => {
                try {
                    await adapter.observeAssistantOutput(event, context);
                }
                catch {
                    // Observability must never alter or block a finalized Pi message.
                }
            });
        }
    };
}
export default function kendrPiOptimizer(pi) {
    try {
        createPiOptimizerExtension(optionsFromEnvironment())(pi);
    }
    catch {
        // Invalid local configuration disables the extension instead of blocking Pi.
    }
}
function optionsFromEnvironment() {
    const options = {};
    if (process.env.KENDR_OPTIMIZER_ENDPOINT) {
        options.coreEndpoint = process.env.KENDR_OPTIMIZER_ENDPOINT;
    }
    if (process.env.KENDR_OPTIMIZER_TIMEOUT_MS) {
        const timeoutMs = Number(process.env.KENDR_OPTIMIZER_TIMEOUT_MS);
        if (Number.isSafeInteger(timeoutMs) && timeoutMs > 0)
            options.timeoutMs = timeoutMs;
    }
    options.shadow = envBoolean(process.env.KENDR_OPTIMIZER_SHADOW);
    options.observeOutput = envBoolean(process.env.KENDR_OPTIMIZER_OBSERVE_OUTPUT);
    return options;
}
function envBoolean(value) {
    return value === "1" || value?.toLowerCase() === "true";
}
function encodeContext(sourceMessages) {
    const messages = [];
    const bindings = [];
    for (let sourceIndex = 0; sourceIndex < sourceMessages.length; sourceIndex += 1) {
        const source = sourceMessages[sourceIndex];
        if (!isRecord(source))
            continue;
        const role = mapPiRole(source.role);
        if (!role)
            continue;
        const parts = [];
        const pending = [];
        const toolCallId = nonEmptyString(source.toolCallId) ?? "pi-tool-result-" + sourceIndex;
        const toolName = nonEmptyString(source.toolName);
        const isError = source.isError === true;
        if (typeof source.content === "string" && source.content.length > 0) {
            const coreType = role === "tool" ? "tool_result" : "text";
            const partIndex = parts.length;
            if (coreType === "tool_result") {
                const callId = toolCallId + ":0";
                parts.push({
                    type: "tool_result",
                    call_id: callId,
                    name: toolName,
                    content: source.content,
                    is_error: isError,
                });
                pending.push({
                    sourceMessageIndex: sourceIndex,
                    sourcePath: ["content"],
                    corePartIndex: partIndex,
                    coreType,
                    callId,
                    original: source.content,
                });
            }
            else {
                parts.push({ type: "text", text: source.content });
                pending.push({
                    sourceMessageIndex: sourceIndex,
                    sourcePath: ["content"],
                    corePartIndex: partIndex,
                    coreType,
                    original: source.content,
                });
            }
        }
        else if (Array.isArray(source.content)) {
            let toolTextIndex = 0;
            for (let blockIndex = 0; blockIndex < source.content.length; blockIndex += 1) {
                const block = source.content[blockIndex];
                if (!isRecord(block) || block.type !== "text" || typeof block.text !== "string")
                    continue;
                if (block.text.length === 0)
                    continue;
                const coreType = role === "tool" ? "tool_result" : "text";
                const partIndex = parts.length;
                if (coreType === "tool_result") {
                    const callId = toolCallId + ":" + toolTextIndex;
                    toolTextIndex += 1;
                    parts.push({
                        type: "tool_result",
                        call_id: callId,
                        name: toolName,
                        content: block.text,
                        is_error: isError,
                    });
                    pending.push({
                        sourceMessageIndex: sourceIndex,
                        sourcePath: ["content", blockIndex, "text"],
                        corePartIndex: partIndex,
                        coreType,
                        callId,
                        original: block.text,
                    });
                }
                else {
                    parts.push({ type: "text", text: block.text });
                    pending.push({
                        sourceMessageIndex: sourceIndex,
                        sourcePath: ["content", blockIndex, "text"],
                        corePartIndex: partIndex,
                        coreType,
                        original: block.text,
                    });
                }
            }
        }
        if (parts.length === 0)
            continue;
        const coreMessageIndex = messages.length;
        messages.push({
            id: "pi-message-" + sourceIndex,
            role,
            parts,
            metadata: { source_role: String(source.role), source_index: sourceIndex },
        });
        for (const binding of pending)
            bindings.push({ ...binding, coreMessageIndex });
    }
    return { messages, bindings };
}
function decodeContextOutcome(raw, requestId, encoded) {
    const outcome = parseLocallyAppliedOutcome(raw, requestId);
    if (!outcome || outcome.content.messages.length !== encoded.messages.length)
        return undefined;
    const replacements = [];
    for (let index = 0; index < encoded.bindings.length; index += 1) {
        const binding = encoded.bindings[index];
        if (!binding)
            return undefined;
        const expectedMessage = encoded.messages[binding.coreMessageIndex];
        const message = outcome.content.messages[binding.coreMessageIndex];
        if (!expectedMessage || !isRecord(message))
            return undefined;
        if (message.id !== expectedMessage.id || message.role !== expectedMessage.role)
            return undefined;
        if (!Array.isArray(message.parts) || message.parts.length !== expectedMessage.parts.length) {
            return undefined;
        }
        const part = message.parts[binding.corePartIndex];
        if (!isRecord(part) || part.type !== binding.coreType)
            return undefined;
        if (binding.coreType === "tool_result") {
            if (part.call_id !== binding.callId || typeof part.content !== "string")
                return undefined;
            replacements.push(part.content);
        }
        else {
            if (typeof part.text !== "string")
                return undefined;
            replacements.push(part.text);
        }
    }
    return replacements;
}
function applyContextBindings(sourceMessages, bindings, replacements) {
    if (bindings.length !== replacements.length)
        return undefined;
    const copy = structuredClone(sourceMessages);
    let changed = false;
    for (let index = 0; index < bindings.length; index += 1) {
        const binding = bindings[index];
        const replacement = replacements[index];
        if (!binding || replacement === undefined)
            return undefined;
        const message = copy[binding.sourceMessageIndex];
        if (!setAtPath(message, binding.sourcePath, replacement))
            return undefined;
        if (replacement !== binding.original)
            changed = true;
    }
    return changed ? copy : undefined;
}
function decodeSingleText(raw, requestId, messageId, role, original) {
    const outcome = parseLocallyAppliedOutcome(raw, requestId);
    if (!outcome || outcome.content.messages.length !== 1)
        return undefined;
    const message = outcome.content.messages[0];
    if (!isRecord(message) || message.id !== messageId || message.role !== role)
        return undefined;
    if (!Array.isArray(message.parts) || message.parts.length !== 1)
        return undefined;
    const part = message.parts[0];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string")
        return undefined;
    return part.text === original ? undefined : part.text;
}
function decodeToolParts(raw, requestId, messageId, bindings) {
    const outcome = parseLocallyAppliedOutcome(raw, requestId);
    if (!outcome || outcome.content.messages.length !== 1)
        return undefined;
    const message = outcome.content.messages[0];
    if (!isRecord(message) || message.id !== messageId || message.role !== "tool")
        return undefined;
    if (!Array.isArray(message.parts) || message.parts.length !== bindings.length)
        return undefined;
    const replacements = [];
    for (let index = 0; index < bindings.length; index += 1) {
        const binding = bindings[index];
        const part = message.parts[index];
        if (!binding || !isRecord(part) || part.type !== "tool_result")
            return undefined;
        if (part.call_id !== binding.callId || typeof part.content !== "string")
            return undefined;
        replacements.push(part.content);
    }
    return replacements;
}
function parseLocallyAppliedOutcome(raw, requestId) {
    if (!isRecord(raw) || !isRecord(raw.content) || !Array.isArray(raw.content.messages)) {
        return undefined;
    }
    if (!isRecord(raw.receipt))
        return undefined;
    const receipt = raw.receipt;
    const original = receipt.original;
    const optimized = receipt.optimized;
    if (!isRecord(original) || !isRecord(optimized))
        return undefined;
    if (receipt.schema_version !== "kendr.receipt/v1" ||
        receipt.request_id !== requestId ||
        receipt.status !== "applied" ||
        typeof receipt.verified_savings !== "boolean" ||
        !Number.isSafeInteger(receipt.token_delta) ||
        receipt.token_delta <= 0 ||
        !Number.isSafeInteger(original.tokens) ||
        !Number.isSafeInteger(optimized.tokens) ||
        original.tokens <= optimized.tokens) {
        return undefined;
    }
    return raw;
}
function mapPiRole(value) {
    if (value === "system" || value === "developer" || value === "user" || value === "assistant") {
        return value;
    }
    if (value === "toolResult" || value === "tool")
        return "tool";
    return undefined;
}
function safeSessionId(context) {
    try {
        const manager = context.sessionManager;
        return nonEmptyString(manager.getSessionId?.());
    }
    catch {
        return undefined;
    }
}
function setAtPath(root, path, value) {
    if (path.length === 0)
        return false;
    let cursor = root;
    for (let index = 0; index < path.length - 1; index += 1) {
        const segment = path[index];
        if (segment === undefined || (!isRecord(cursor) && !Array.isArray(cursor)))
            return false;
        cursor = cursor[segment];
    }
    const last = path[path.length - 1];
    if (last === undefined || (!isRecord(cursor) && !Array.isArray(cursor)))
        return false;
    cursor[last] = value;
    return true;
}
function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}
function nonEmptyString(value) {
    return typeof value === "string" && value.length > 0 ? value : undefined;
}
function boundedPositiveInteger(value, fallback, max) {
    return Number.isSafeInteger(value) && value !== undefined && value > 0 && value <= max
        ? value
        : fallback;
}
//# sourceMappingURL=index.js.map