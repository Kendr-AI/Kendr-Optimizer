import type { Hooks, Plugin, PluginOptions } from "@opencode-ai/plugin";

export const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export const OPENCODE_AUDIT = Object.freeze({
  package: "@opencode-ai/plugin",
  version: "1.18.15",
  commit: "d7b115f623760e68a4749d16508a9eca350f246f",
  api: "v1",
});

export type FetchLike = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

export interface OpenCodeOptimizerOptions {
  coreEndpoint?: string;
  timeoutMs?: number;
  shadow?: boolean;
  experimentalHistory?: boolean;
  experimentalSystem?: boolean;
  fetch?: FetchLike;
}

type JsonRecord = Record<string, unknown>;

interface CorePart extends JsonRecord {
  type: string;
}

interface CoreMessage extends JsonRecord {
  id: string;
  role: "system" | "developer" | "user" | "assistant" | "tool";
  parts: CorePart[];
}

interface PartSource {
  role: unknown;
  parts: unknown[];
}

interface PartBinding {
  sourceMessageIndex: number;
  sourcePartIndex: number;
  coreMessageIndex: number;
  corePartIndex: number;
  original: string;
}

interface EncodedParts {
  messages: CoreMessage[];
  bindings: PartBinding[];
}

interface CoreOutcome {
  content: { messages: unknown[] };
  receipt: {
    schema_version: string;
    request_id: string;
    status: string;
    token_delta: number;
    verified_savings: boolean;
    original: { tokens: number };
    optimized: { tokens: number };
  };
}

export class LoopbackEndpointError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LoopbackEndpointError";
  }
}

export function validateNumericLoopbackEndpoint(raw: string): URL {
  let endpoint: URL;
  try {
    endpoint = new URL(raw);
  } catch {
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

export class KendrOpenCodeAdapter {
  readonly #endpoint: URL;
  readonly #timeoutMs: number;
  readonly #shadow: boolean;
  readonly #fetch: FetchLike;

  constructor(options: OpenCodeOptimizerOptions = {}) {
    this.#endpoint = validateNumericLoopbackEndpoint(
      options.coreEndpoint ?? DEFAULT_CORE_ENDPOINT,
    );
    this.#timeoutMs = boundedPositiveInteger(options.timeoutMs, 100, 10_000);
    this.#shadow = options.shadow === true;
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  async optimizeCurrentMessage(
    parts: unknown[],
    sessionId: string,
    messageId: string | undefined,
    model: { providerID: string; modelID: string } | undefined,
  ): Promise<unknown[] | undefined> {
    const source: PartSource[] = [{ role: "user", parts }];
    const encoded = encodePartSources(source, "opencode-current");
    if (encoded.bindings.length === 0) return undefined;
    const requestId = "opencode-chat-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "request",
      requestId,
      encoded.messages,
      sessionId,
      model ? model.providerID + "/" + model.modelID : undefined,
      { seam: "chat.message", message_id: messageId ?? null },
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return undefined;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return undefined;
    const applied = applyPartBindings(source, encoded.bindings, replacements);
    return applied?.[0]?.parts;
  }

  async optimizeToolOutput(
    output: string,
    input: { tool: string; sessionID: string; callID: string },
  ): Promise<string | undefined> {
    if (output.length === 0) return undefined;
    const requestId = "opencode-tool-result-" + crypto.randomUUID();
    const messageId = "opencode-tool-message";
    const callId = input.callID + ":0";
    const request = this.#buildRequest(
      "tool_result",
      requestId,
      [
        {
          id: messageId,
          role: "tool",
          parts: [
            {
              type: "tool_result",
              call_id: callId,
              name: input.tool,
              content: output,
              is_error: false,
            },
          ],
          metadata: { host: "opencode", seam: "tool.execute.after" },
        },
      ],
      input.sessionID,
      undefined,
      { seam: "tool.execute.after", tool: input.tool },
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return undefined;
    const outcome = parseLocallyAppliedOutcome(raw, requestId);
    if (!outcome || outcome.content.messages.length !== 1) return undefined;
    const message = outcome.content.messages[0];
    if (!isRecord(message) || message.id !== messageId || message.role !== "tool") return undefined;
    if (!Array.isArray(message.parts) || message.parts.length !== 1) return undefined;
    const part = message.parts[0];
    if (
      !isRecord(part) ||
      part.type !== "tool_result" ||
      part.call_id !== callId ||
      typeof part.content !== "string" ||
      part.content === output
    ) {
      return undefined;
    }
    return part.content;
  }

  async optimizeHistory(messages: unknown[]): Promise<unknown[] | undefined> {
    const sources: PartSource[] = messages.map((entry) => {
      if (!isRecord(entry)) return { role: undefined, parts: [] };
      const info = isRecord(entry.info) ? entry.info : {};
      return {
        role: info.role,
        parts: Array.isArray(entry.parts) ? entry.parts : [],
      };
    });
    const encoded = encodePartSources(sources, "opencode-history");
    if (encoded.bindings.length === 0) return undefined;
    const requestId = "opencode-history-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "history_ingest",
      requestId,
      encoded.messages,
      undefined,
      undefined,
      { seam: "experimental.chat.messages.transform", experimental: true },
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return undefined;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return undefined;
    const applied = applyPartBindings(sources, encoded.bindings, replacements);
    if (!applied) return undefined;

    const copy = structuredClone(messages);
    for (let index = 0; index < copy.length; index += 1) {
      const entry = copy[index];
      const source = applied[index];
      if (isRecord(entry) && source) entry.parts = source.parts;
    }
    return copy;
  }

  async optimizeSystem(
    system: string[],
    sessionId: string | undefined,
    model: unknown,
  ): Promise<string[] | undefined> {
    const sources: PartSource[] = system.map((text) => ({
      role: "system",
      parts: [{ type: "text", text }],
    }));
    const encoded = encodePartSources(sources, "opencode-system");
    if (encoded.bindings.length === 0) return undefined;
    const requestId = "opencode-system-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "request",
      requestId,
      encoded.messages,
      sessionId,
      modelName(model),
      { seam: "experimental.chat.system.transform", experimental: true },
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return undefined;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return undefined;
    const applied = applyPartBindings(sources, encoded.bindings, replacements);
    if (!applied) return undefined;
    return applied.map((source, index) => {
      const part = source.parts[0];
      return isRecord(part) && typeof part.text === "string" ? part.text : system[index] ?? "";
    });
  }

  #buildRequest(
    phase: "request" | "tool_result" | "history_ingest",
    requestId: string,
    messages: CoreMessage[],
    sessionId: string | undefined,
    model: string | undefined,
    metadata: JsonRecord,
  ): JsonRecord {
    return {
      schema_version: "kendr.optimize/v1",
      phase,
      request_id: requestId,
      ...(sessionId ? { session_id: sessionId } : {}),
      content: {
        messages,
        tools: [],
        metadata: {
          adapter: "@kendr/optimizer-opencode",
          host_version: OPENCODE_AUDIT.version,
          provider_egress: false,
          ...metadata,
        },
      },
      target: {
        tokenizer_profile: "approximate",
        ...(model ? { model } : {}),
      },
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
        shadow: this.#shadow,
        enable_tool_selection: false,
        enable_lossy_tool_output: false,
        enable_generation_policy: false,
      },
    };
  }

  async #dispatch(request: JsonRecord): Promise<unknown> {
    return this.#post(this.#shadow ? "/v1/analyze" : "/v1/optimize", request);
  }

  async #post(path: string, request: JsonRecord): Promise<unknown> {
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
      if (!response.ok) return undefined;
      return await response.json();
    } catch {
      return undefined;
    } finally {
      clearTimeout(timer);
    }
  }
}

export function createOpenCodePlugin(options: OpenCodeOptimizerOptions = {}): Plugin {
  return async () => {
    let adapter: KendrOpenCodeAdapter;
    try {
      adapter = new KendrOpenCodeAdapter(options);
    } catch {
      return {};
    }

    const hooks: Hooks = {
      "chat.message": async (input, output) => {
        try {
          const parts = await adapter.optimizeCurrentMessage(
            output.parts as unknown[],
            input.sessionID,
            input.messageID,
            input.model,
          );
          if (parts) output.parts = parts as typeof output.parts;
        } catch {
          // OpenCode V1 propagates hook exceptions, so each hook is self-contained.
        }
      },
      "tool.execute.after": async (input, output) => {
        try {
          const optimized = await adapter.optimizeToolOutput(output.output, input);
          if (optimized !== undefined) output.output = optimized;
        } catch {
          // Leave title, output, and metadata exactly as OpenCode supplied them.
        }
      },
    };

    if (options.experimentalHistory === true) {
      hooks["experimental.chat.messages.transform"] = async (_input, output) => {
        try {
          const messages = await adapter.optimizeHistory(output.messages as unknown[]);
          if (messages) output.messages = messages as typeof output.messages;
        } catch {
          // Experimental hooks receive the same fail-open guard as stable hooks.
        }
      };
    }
    if (options.experimentalSystem === true) {
      hooks["experimental.chat.system.transform"] = async (input, output) => {
        try {
          const system = await adapter.optimizeSystem(output.system, input.sessionID, input.model);
          if (system) output.system = system;
        } catch {
          // Never fail an OpenCode request because a local optimizer is unavailable.
        }
      };
    }
    return hooks;
  };
}

export const KendrOptimizerPlugin: Plugin = async (input, pluginOptions) => {
  const options = optionsFromEnvironmentAndPlugin(pluginOptions);
  return createOpenCodePlugin(options)(input, pluginOptions);
};

function optionsFromEnvironmentAndPlugin(pluginOptions: PluginOptions | undefined): OpenCodeOptimizerOptions {
  const options: OpenCodeOptimizerOptions = {};
  const endpoint = stringOption(pluginOptions, "coreEndpoint") ?? process.env.KENDR_OPTIMIZER_ENDPOINT;
  if (endpoint) options.coreEndpoint = endpoint;
  const timeout = numberOption(pluginOptions, "timeoutMs") ?? numberFromEnv("KENDR_OPTIMIZER_TIMEOUT_MS");
  if (timeout !== undefined) options.timeoutMs = timeout;
  options.shadow = booleanOption(pluginOptions, "shadow") ?? envBoolean(process.env.KENDR_OPTIMIZER_SHADOW);
  options.experimentalHistory =
    booleanOption(pluginOptions, "experimentalHistory") ??
    envBoolean(process.env.KENDR_OPENCODE_EXPERIMENTAL_HISTORY);
  options.experimentalSystem =
    booleanOption(pluginOptions, "experimentalSystem") ??
    envBoolean(process.env.KENDR_OPENCODE_EXPERIMENTAL_SYSTEM);
  return options;
}

function encodePartSources(sources: PartSource[], prefix: string): EncodedParts {
  const messages: CoreMessage[] = [];
  const bindings: PartBinding[] = [];
  for (let sourceIndex = 0; sourceIndex < sources.length; sourceIndex += 1) {
    const source = sources[sourceIndex];
    if (!source) continue;
    const role = mapRole(source.role);
    if (!role) continue;
    const parts: CorePart[] = [];
    const pending: Array<Omit<PartBinding, "coreMessageIndex">> = [];
    for (let partIndex = 0; partIndex < source.parts.length; partIndex += 1) {
      const sourcePart = source.parts[partIndex];
      if (
        !isRecord(sourcePart) ||
        sourcePart.type !== "text" ||
        typeof sourcePart.text !== "string" ||
        sourcePart.text.length === 0
      ) {
        continue;
      }
      const corePartIndex = parts.length;
      parts.push({ type: "text", text: sourcePart.text });
      pending.push({
        sourceMessageIndex: sourceIndex,
        sourcePartIndex: partIndex,
        corePartIndex,
        original: sourcePart.text,
      });
    }
    if (parts.length === 0) continue;
    const coreMessageIndex = messages.length;
    messages.push({
      id: prefix + "-" + sourceIndex,
      role,
      parts,
      metadata: { source_index: sourceIndex },
    });
    for (const binding of pending) bindings.push({ ...binding, coreMessageIndex });
  }
  return { messages, bindings };
}

function decodePartOutcome(
  raw: unknown,
  requestId: string,
  encoded: EncodedParts,
): string[] | undefined {
  const outcome = parseLocallyAppliedOutcome(raw, requestId);
  if (!outcome || outcome.content.messages.length !== encoded.messages.length) return undefined;
  const replacements: string[] = [];
  for (const binding of encoded.bindings) {
    const expectedMessage = encoded.messages[binding.coreMessageIndex];
    const message = outcome.content.messages[binding.coreMessageIndex];
    if (!expectedMessage || !isRecord(message)) return undefined;
    if (message.id !== expectedMessage.id || message.role !== expectedMessage.role) return undefined;
    if (!Array.isArray(message.parts) || message.parts.length !== expectedMessage.parts.length) {
      return undefined;
    }
    const part = message.parts[binding.corePartIndex];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string") return undefined;
    replacements.push(part.text);
  }
  return replacements;
}

function applyPartBindings(
  sources: PartSource[],
  bindings: PartBinding[],
  replacements: string[],
): PartSource[] | undefined {
  if (bindings.length !== replacements.length) return undefined;
  const copy = structuredClone(sources);
  let changed = false;
  for (let index = 0; index < bindings.length; index += 1) {
    const binding = bindings[index];
    const replacement = replacements[index];
    if (!binding || replacement === undefined) return undefined;
    const part = copy[binding.sourceMessageIndex]?.parts[binding.sourcePartIndex];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string") return undefined;
    if (part.text !== replacement) changed = true;
    part.text = replacement;
  }
  return changed ? copy : undefined;
}

function parseLocallyAppliedOutcome(raw: unknown, requestId: string): CoreOutcome | undefined {
  if (!isRecord(raw) || !isRecord(raw.content) || !Array.isArray(raw.content.messages)) {
    return undefined;
  }
  if (!isRecord(raw.receipt)) return undefined;
  const receipt = raw.receipt;
  const original = receipt.original;
  const optimized = receipt.optimized;
  if (!isRecord(original) || !isRecord(optimized)) return undefined;
  if (
    receipt.schema_version !== "kendr.receipt/v1" ||
    receipt.request_id !== requestId ||
    receipt.status !== "applied" ||
    typeof receipt.verified_savings !== "boolean" ||
    !Number.isSafeInteger(receipt.token_delta) ||
    (receipt.token_delta as number) <= 0 ||
    !Number.isSafeInteger(original.tokens) ||
    !Number.isSafeInteger(optimized.tokens) ||
    (original.tokens as number) <= (optimized.tokens as number)
  ) {
    return undefined;
  }
  return raw as unknown as CoreOutcome;
}

function mapRole(value: unknown): CoreMessage["role"] | undefined {
  if (value === "system" || value === "developer" || value === "user" || value === "assistant") {
    return value;
  }
  if (value === "tool" || value === "toolResult") return "tool";
  return undefined;
}

function modelName(model: unknown): string | undefined {
  if (!isRecord(model)) return undefined;
  const provider = typeof model.providerID === "string" ? model.providerID : undefined;
  const id =
    typeof model.modelID === "string"
      ? model.modelID
      : typeof model.id === "string"
        ? model.id
        : undefined;
  return provider && id ? provider + "/" + id : id;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function boundedPositiveInteger(value: number | undefined, fallback: number, max: number): number {
  return Number.isSafeInteger(value) && value !== undefined && value > 0 && value <= max
    ? value
    : fallback;
}

function stringOption(options: PluginOptions | undefined, key: string): string | undefined {
  const value = options?.[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberOption(options: PluginOptions | undefined, key: string): number | undefined {
  const value = options?.[key];
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function booleanOption(options: PluginOptions | undefined, key: string): boolean | undefined {
  const value = options?.[key];
  return typeof value === "boolean" ? value : undefined;
}

function numberFromEnv(name: string): number | undefined {
  const raw = process.env[name];
  if (!raw) return undefined;
  const value = Number(raw);
  return Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function envBoolean(value: string | undefined): boolean {
  return value === "1" || value?.toLowerCase() === "true";
}
