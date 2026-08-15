// Installed by Kendr Optimizer. This bundle exports one OpenCode plugin factory.

// src/index.ts
var DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
var OPENCODE_AUDIT = Object.freeze({
  package: "@opencode-ai/plugin",
  version: "1.18.15",
  commit: "d7b115f623760e68a4749d16508a9eca350f246f",
  api: "v1"
});
var LoopbackEndpointError = class extends Error {
  constructor(message) {
    super(message);
    this.name = "LoopbackEndpointError";
  }
};
function validateNumericLoopbackEndpoint(raw) {
  let endpoint;
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
var KendrOpenCodeAdapter = class {
  #endpoint;
  #timeoutMs;
  #shadow;
  #fetch;
  constructor(options = {}) {
    this.#endpoint = validateNumericLoopbackEndpoint(
      options.coreEndpoint ?? DEFAULT_CORE_ENDPOINT
    );
    this.#timeoutMs = boundedPositiveInteger(options.timeoutMs, 100, 1e4);
    this.#shadow = options.shadow === true;
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  }
  async optimizeCurrentMessage(parts, sessionId, messageId, model) {
    const source = [{ role: "user", parts }];
    const encoded = encodePartSources(source, "opencode-current");
    if (encoded.bindings.length === 0) return void 0;
    const requestId = "opencode-chat-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "request",
      requestId,
      encoded.messages,
      sessionId,
      model ? model.providerID + "/" + model.modelID : void 0,
      { seam: "chat.message", message_id: messageId ?? null }
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return void 0;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return void 0;
    const applied = applyPartBindings(source, encoded.bindings, replacements);
    return applied?.[0]?.parts;
  }
  async optimizeToolOutput(output, input) {
    if (output.length === 0) return void 0;
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
              is_error: false
            }
          ],
          metadata: { host: "opencode", seam: "tool.execute.after" }
        }
      ],
      input.sessionID,
      void 0,
      { seam: "tool.execute.after", tool: input.tool }
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return void 0;
    const outcome = parseLocallyAppliedOutcome(raw, requestId);
    if (!outcome || outcome.content.messages.length !== 1) return void 0;
    const message = outcome.content.messages[0];
    if (!isRecord(message) || message.id !== messageId || message.role !== "tool") return void 0;
    if (!Array.isArray(message.parts) || message.parts.length !== 1) return void 0;
    const part = message.parts[0];
    if (!isRecord(part) || part.type !== "tool_result" || part.call_id !== callId || typeof part.content !== "string" || part.content === output) {
      return void 0;
    }
    return part.content;
  }
  async optimizeHistory(messages) {
    const sources = messages.map((entry) => {
      if (!isRecord(entry)) return { role: void 0, parts: [] };
      const info = isRecord(entry.info) ? entry.info : {};
      return {
        role: info.role,
        parts: Array.isArray(entry.parts) ? entry.parts : []
      };
    });
    const encoded = encodePartSources(sources, "opencode-history");
    if (encoded.bindings.length === 0) return void 0;
    const requestId = "opencode-history-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "history_ingest",
      requestId,
      encoded.messages,
      void 0,
      void 0,
      { seam: "experimental.chat.messages.transform", experimental: true }
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return void 0;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return void 0;
    const applied = applyPartBindings(sources, encoded.bindings, replacements);
    if (!applied) return void 0;
    const copy = structuredClone(messages);
    for (let index = 0; index < copy.length; index += 1) {
      const entry = copy[index];
      const source = applied[index];
      if (isRecord(entry) && source) entry.parts = source.parts;
    }
    return copy;
  }
  async optimizeSystem(system, sessionId, model) {
    const sources = system.map((text) => ({
      role: "system",
      parts: [{ type: "text", text }]
    }));
    const encoded = encodePartSources(sources, "opencode-system");
    if (encoded.bindings.length === 0) return void 0;
    const requestId = "opencode-system-" + crypto.randomUUID();
    const request = this.#buildRequest(
      "request",
      requestId,
      encoded.messages,
      sessionId,
      modelName(model),
      { seam: "experimental.chat.system.transform", experimental: true }
    );
    const raw = await this.#dispatch(request);
    if (this.#shadow) return void 0;
    const replacements = decodePartOutcome(raw, requestId, encoded);
    if (!replacements) return void 0;
    const applied = applyPartBindings(sources, encoded.bindings, replacements);
    if (!applied) return void 0;
    return applied.map((source, index) => {
      const part = source.parts[0];
      return isRecord(part) && typeof part.text === "string" ? part.text : system[index] ?? "";
    });
  }
  #buildRequest(phase, requestId, messages, sessionId, model, metadata) {
    return {
      schema_version: "kendr.optimize/v1",
      phase,
      request_id: requestId,
      ...sessionId ? { session_id: sessionId } : {},
      content: {
        messages,
        tools: [],
        metadata: {
          adapter: "@kendr/optimizer-opencode",
          host_version: OPENCODE_AUDIT.version,
          provider_egress: false,
          ...metadata
        }
      },
      target: {
        tokenizer_profile: "approximate",
        ...model ? { model } : {}
      },
      host_capabilities: {
        can_narrow_tools: false,
        can_restore_references: false,
        can_retry_with_full_tools: false,
        streaming_output: true,
        can_set_max_output_tokens: false,
        can_set_verbosity: false,
        can_append_generation_policy: false
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
        enable_generation_policy: false
      }
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
          accept: "application/json"
        },
        body: JSON.stringify(request),
        signal: controller.signal
      });
      if (!response.ok) return void 0;
      return await response.json();
    } catch {
      return void 0;
    } finally {
      clearTimeout(timer);
    }
  }
};
function createOpenCodePlugin(options = {}) {
  return async () => {
    let adapter;
    try {
      adapter = new KendrOpenCodeAdapter(options);
    } catch {
      return {};
    }
    const hooks = {
      "chat.message": async (input, output) => {
        try {
          const parts = await adapter.optimizeCurrentMessage(
            output.parts,
            input.sessionID,
            input.messageID,
            input.model
          );
          if (parts) output.parts = parts;
        } catch {
        }
      },
      "tool.execute.after": async (input, output) => {
        try {
          const optimized = await adapter.optimizeToolOutput(output.output, input);
          if (optimized !== void 0) output.output = optimized;
        } catch {
        }
      }
    };
    if (options.experimentalHistory === true) {
      hooks["experimental.chat.messages.transform"] = async (_input, output) => {
        try {
          const messages = await adapter.optimizeHistory(output.messages);
          if (messages) output.messages = messages;
        } catch {
        }
      };
    }
    if (options.experimentalSystem === true) {
      hooks["experimental.chat.system.transform"] = async (input, output) => {
        try {
          const system = await adapter.optimizeSystem(output.system, input.sessionID, input.model);
          if (system) output.system = system;
        } catch {
        }
      };
    }
    return hooks;
  };
}
var KendrOptimizerPlugin = async (input, pluginOptions) => {
  const options = optionsFromEnvironmentAndPlugin(pluginOptions);
  return createOpenCodePlugin(options)(input, pluginOptions);
};
function optionsFromEnvironmentAndPlugin(pluginOptions) {
  const options = {};
  const endpoint = stringOption(pluginOptions, "coreEndpoint") ?? process.env.KENDR_OPTIMIZER_ENDPOINT;
  if (endpoint) options.coreEndpoint = endpoint;
  const timeout = numberOption(pluginOptions, "timeoutMs") ?? numberFromEnv("KENDR_OPTIMIZER_TIMEOUT_MS");
  if (timeout !== void 0) options.timeoutMs = timeout;
  options.shadow = booleanOption(pluginOptions, "shadow") ?? envBoolean(process.env.KENDR_OPTIMIZER_SHADOW);
  options.experimentalHistory = booleanOption(pluginOptions, "experimentalHistory") ?? envBoolean(process.env.KENDR_OPENCODE_EXPERIMENTAL_HISTORY);
  options.experimentalSystem = booleanOption(pluginOptions, "experimentalSystem") ?? envBoolean(process.env.KENDR_OPENCODE_EXPERIMENTAL_SYSTEM);
  return options;
}
function encodePartSources(sources, prefix) {
  const messages = [];
  const bindings = [];
  for (let sourceIndex = 0; sourceIndex < sources.length; sourceIndex += 1) {
    const source = sources[sourceIndex];
    if (!source) continue;
    const role = mapRole(source.role);
    if (!role) continue;
    const parts = [];
    const pending = [];
    for (let partIndex = 0; partIndex < source.parts.length; partIndex += 1) {
      const sourcePart = source.parts[partIndex];
      if (!isRecord(sourcePart) || sourcePart.type !== "text" || typeof sourcePart.text !== "string" || sourcePart.text.length === 0) {
        continue;
      }
      const corePartIndex = parts.length;
      parts.push({ type: "text", text: sourcePart.text });
      pending.push({
        sourceMessageIndex: sourceIndex,
        sourcePartIndex: partIndex,
        corePartIndex,
        original: sourcePart.text
      });
    }
    if (parts.length === 0) continue;
    const coreMessageIndex = messages.length;
    messages.push({
      id: prefix + "-" + sourceIndex,
      role,
      parts,
      metadata: { source_index: sourceIndex }
    });
    for (const binding of pending) bindings.push({ ...binding, coreMessageIndex });
  }
  return { messages, bindings };
}
function decodePartOutcome(raw, requestId, encoded) {
  const outcome = parseLocallyAppliedOutcome(raw, requestId);
  if (!outcome || outcome.content.messages.length !== encoded.messages.length) return void 0;
  const replacements = [];
  for (const binding of encoded.bindings) {
    const expectedMessage = encoded.messages[binding.coreMessageIndex];
    const message = outcome.content.messages[binding.coreMessageIndex];
    if (!expectedMessage || !isRecord(message)) return void 0;
    if (message.id !== expectedMessage.id || message.role !== expectedMessage.role) return void 0;
    if (!Array.isArray(message.parts) || message.parts.length !== expectedMessage.parts.length) {
      return void 0;
    }
    const part = message.parts[binding.corePartIndex];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string") return void 0;
    replacements.push(part.text);
  }
  return replacements;
}
function applyPartBindings(sources, bindings, replacements) {
  if (bindings.length !== replacements.length) return void 0;
  const copy = structuredClone(sources);
  let changed = false;
  for (let index = 0; index < bindings.length; index += 1) {
    const binding = bindings[index];
    const replacement = replacements[index];
    if (!binding || replacement === void 0) return void 0;
    const part = copy[binding.sourceMessageIndex]?.parts[binding.sourcePartIndex];
    if (!isRecord(part) || part.type !== "text" || typeof part.text !== "string") return void 0;
    if (part.text !== replacement) changed = true;
    part.text = replacement;
  }
  return changed ? copy : void 0;
}
function parseLocallyAppliedOutcome(raw, requestId) {
  if (!isRecord(raw) || !isRecord(raw.content) || !Array.isArray(raw.content.messages)) {
    return void 0;
  }
  if (!isRecord(raw.receipt)) return void 0;
  const receipt = raw.receipt;
  const original = receipt.original;
  const optimized = receipt.optimized;
  if (!isRecord(original) || !isRecord(optimized)) return void 0;
  if (receipt.schema_version !== "kendr.receipt/v1" || receipt.request_id !== requestId || receipt.status !== "applied" || typeof receipt.verified_savings !== "boolean" || !Number.isSafeInteger(receipt.token_delta) || receipt.token_delta <= 0 || !Number.isSafeInteger(original.tokens) || !Number.isSafeInteger(optimized.tokens) || original.tokens <= optimized.tokens) {
    return void 0;
  }
  return raw;
}
function mapRole(value) {
  if (value === "system" || value === "developer" || value === "user" || value === "assistant") {
    return value;
  }
  if (value === "tool" || value === "toolResult") return "tool";
  return void 0;
}
function modelName(model) {
  if (!isRecord(model)) return void 0;
  const provider = typeof model.providerID === "string" ? model.providerID : void 0;
  const id = typeof model.modelID === "string" ? model.modelID : typeof model.id === "string" ? model.id : void 0;
  return provider && id ? provider + "/" + id : id;
}
function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function boundedPositiveInteger(value, fallback, max) {
  return Number.isSafeInteger(value) && value !== void 0 && value > 0 && value <= max ? value : fallback;
}
function stringOption(options, key) {
  const value = options?.[key];
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
function numberOption(options, key) {
  const value = options?.[key];
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : void 0;
}
function booleanOption(options, key) {
  const value = options?.[key];
  return typeof value === "boolean" ? value : void 0;
}
function numberFromEnv(name) {
  const raw = process.env[name];
  if (!raw) return void 0;
  const value = Number(raw);
  return Number.isSafeInteger(value) && value > 0 ? value : void 0;
}
function envBoolean(value) {
  return value === "1" || value?.toLowerCase() === "true";
}
export {
  KendrOptimizerPlugin
};
