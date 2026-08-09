import { randomUUID } from "node:crypto";
export const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export const CLAUDE_CHANNELS_AUDIT = Object.freeze({
    version: "2.1.224",
    commit: "66edf5358349356774812264b75b8ea792f0d0a3",
    status: "research_preview",
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
export class KendrClaudeChannelOptimizer {
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
    async prepareNotification(notification, context) {
        if (context.senderAuthorized !== true) {
            return { notification, applied: false, reason: "sender_not_authorized" };
        }
        if (notification.content.length === 0) {
            return { notification, applied: false, reason: "empty_content" };
        }
        const requestId = "claude-channel-" + randomUUID();
        const messageId = "channel-message-" + requestId;
        const request = buildRequest(notification.content, context, requestId, messageId, this.#shadow);
        const raw = await this.#post(this.#shadow ? "/v1/analyze" : "/v1/optimize", request);
        if (this.#shadow) {
            return { notification, applied: false, reason: "shadow_only", requestId };
        }
        if (raw === undefined) {
            return { notification, applied: false, reason: "optimizer_unavailable", requestId };
        }
        const replacement = decodeTextOutcome(raw, requestId, messageId);
        if (replacement === undefined) {
            return { notification, applied: false, reason: "not_applied_or_invalid", requestId };
        }
        if (replacement === notification.content) {
            return { notification, applied: false, reason: "unchanged", requestId };
        }
        const optimized = { ...notification, content: replacement };
        return { notification: optimized, applied: true, reason: "optimized", requestId };
    }
    async #post(path, body) {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), this.#timeoutMs);
        try {
            const response = await this.#fetch(new URL(path, this.#endpoint), {
                method: "POST",
                headers: {
                    "content-type": "application/json",
                    accept: "application/json",
                },
                body: JSON.stringify(body),
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
export function createClaudeChannelOptimizer(options = {}) {
    return new KendrClaudeChannelOptimizer(options);
}
function buildRequest(content, context, requestId, messageId, shadow) {
    return {
        schema_version: "kendr.optimize/v1",
        phase: "request",
        request_id: requestId,
        ...(context.sessionId ? { session_id: context.sessionId } : {}),
        content: {
            messages: [
                {
                    id: messageId,
                    role: "user",
                    parts: [{ type: "text", text: content }],
                    metadata: {
                        host: "claude-code-channels",
                        ...(context.channelName ? { channel_name: context.channelName } : {}),
                        ...(context.senderClass ? { sender_class: context.senderClass } : {}),
                        sender_authorized: true,
                    },
                },
            ],
            tools: [],
            metadata: {
                adapter: "@kendr/optimizer-claude-channels",
                host_version: CLAUDE_CHANNELS_AUDIT.version,
                provider_egress: false,
            },
        },
        target: { tokenizer_profile: "approximate" },
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
            shadow,
            enable_tool_selection: false,
            enable_lossy_tool_output: false,
            enable_generation_policy: false,
        },
    };
}
function decodeTextOutcome(raw, requestId, messageId) {
    if (!isRecord(raw) || !isRecord(raw.receipt) || !isRecord(raw.content))
        return undefined;
    if (!isRecord(raw.receipt.original) || !isRecord(raw.receipt.optimized))
        return undefined;
    if (raw.receipt.schema_version !== "kendr.receipt/v1" ||
        raw.receipt.request_id !== requestId ||
        raw.receipt.status !== "applied" ||
        typeof raw.receipt.verified_savings !== "boolean" ||
        !Number.isSafeInteger(raw.receipt.token_delta) ||
        raw.receipt.token_delta <= 0 ||
        !Number.isSafeInteger(raw.receipt.original.tokens) ||
        !Number.isSafeInteger(raw.receipt.optimized.tokens) ||
        raw.receipt.original.tokens <= raw.receipt.optimized.tokens) {
        return undefined;
    }
    if (!Array.isArray(raw.content.messages) || raw.content.messages.length !== 1)
        return undefined;
    const message = raw.content.messages[0];
    if (!isRecord(message) || message.id !== messageId || message.role !== "user")
        return undefined;
    if (!Array.isArray(message.parts) || message.parts.length !== 1)
        return undefined;
    const part = message.parts[0];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string")
        return undefined;
    return part.text;
}
function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}
function boundedPositiveInteger(value, fallback, max) {
    return Number.isSafeInteger(value) && value !== undefined && value > 0 && value <= max
        ? value
        : fallback;
}
//# sourceMappingURL=index.js.map