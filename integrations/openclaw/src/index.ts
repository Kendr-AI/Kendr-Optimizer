import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

const PLUGIN_ID = "kendr-optimizer";
const CONTRACT_VERSION = "kendr.optimize/v1";
const DEFAULT_ENDPOINT = "http://127.0.0.1:7331";

type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

type UnknownRecord = Record<string, unknown>;
type AgentMessage = UnknownRecord;

interface PluginLogger {
  debug(message: string): void;
  info(message: string): void;
  warn(message: string): void;
  error(message: string): void;
}

interface ContextEngineFactoryContext {
  config?: UnknownRecord;
  agentDir?: string;
  workspaceDir?: string;
}

interface OpenClawPluginApi {
  pluginConfig?: UnknownRecord;
  logger: PluginLogger;
  registerContextEngine(
    id: string,
    factory: (context: ContextEngineFactoryContext) => ContextEngine,
  ): void;
}

interface AssembleParams {
  sessionId: string;
  sessionKey?: string;
  messages: AgentMessage[];
  tokenBudget?: number;
  model?: string;
  prompt?: string;
  runtimeSettings?: {
    model?: {
      resolved?: string | null;
    };
    limits?: {
      promptTokenBudget?: number | null;
    };
  };
}

interface ContextEngine {
  readonly info: {
    id: string;
    name: string;
    version: string;
    ownsCompaction: false;
    acceptedHostParams: string[];
    transcriptSemantics: {
      currentTurnFence: "before-current-turn-entry-v1";
      turnAdvancementIdempotency: "atomic-idempotent-v1";
    };
  };
  ingest(params: { sessionId: string; message: AgentMessage }): Promise<{ ingested: boolean }>;
  assemble(params: AssembleParams): Promise<{
    messages: AgentMessage[];
    estimatedTokens: number;
    promptAuthority: "assembled";
  }>;
  compact(params: {
    sessionId: string;
    force?: boolean;
    abortSignal?: AbortSignal;
  }): Promise<{ ok: boolean; compacted: boolean; reason: string }>;
  commitTurn(params: { advancementKey: string }): Promise<{
    status: "committed" | "duplicate";
  }>;
}

type TokenizerProfile = "approximate" | "cl100k_base" | "o200k_base";
type RiskCeiling = "pass_through" | "representation_safe";

interface AdapterConfig {
  endpoint: string;
  timeoutMs: number;
  failureBackoffMs: number;
  tokenizerProfile: TokenizerProfile;
  riskCeiling: RiskCeiling;
  minGainTokens: number;
  minGainPercent: number;
  preserveRecentMessages: number;
  maxToolResultChars: number;
  shadow: boolean;
}

interface KendrTextPart {
  type: "text";
  text: string;
}

interface KendrToolResultPart {
  type: "tool_result";
  call_id: string;
  name?: string;
  content: string;
  is_error: boolean;
}

interface KendrToolCallPart {
  type: "tool_call";
  id: string;
  name: string;
  arguments: JsonValue;
}

interface KendrJsonPart {
  type: "json";
  value: JsonValue;
}

type KendrPart =
  | KendrTextPart
  | KendrToolResultPart
  | KendrToolCallPart
  | KendrJsonPart;

type KendrRole = "system" | "developer" | "user" | "assistant" | "tool";

interface KendrMessage {
  id: string;
  role: KendrRole;
  parts: KendrPart[];
  metadata: Record<string, JsonValue>;
}

interface KendrOptimizeRequest {
  schema_version: typeof CONTRACT_VERSION;
  phase: "request";
  request_id: string;
  session_id: string;
  content: {
    messages: KendrMessage[];
    tools: [];
    output_contract: null;
    metadata: Record<string, JsonValue>;
  };
  target: {
    tokenizer_profile: TokenizerProfile;
    model: string | null;
    context_limit: number | null;
    pricing: null;
    cache_segments: [];
  };
  host_capabilities: {
    can_narrow_tools: false;
    can_restore_references: false;
    can_retry_with_full_tools: false;
    streaming_output: true;
  };
  policy: {
    risk_ceiling: RiskCeiling;
    min_gain_tokens: number;
    min_gain_percent: number;
    latency_budget_ms: number;
    preserve_cache_prefix: true;
    shadow: boolean;
    preserve_recent_messages: number;
    max_tool_result_chars: number;
    enable_tool_selection: false;
    enable_lossy_tool_output: false;
    enabled_engines: [];
  };
}

interface KendrReceipt {
  status: "applied" | "skipped" | "shadow" | "reverted";
  original: { tokens: number };
  optimized: { tokens: number };
  token_delta: number;
}

interface KendrOutcome {
  content: {
    messages: KendrMessage[];
  };
  receipt: KendrReceipt;
}

type PartBinding =
  | {
      kind: "content_string";
      original: KendrTextPart | KendrToolResultPart;
    }
  | { kind: "block_text"; blockIndex: number }
  | {
      kind: "block_tool_result";
      blockIndex: number;
      original: KendrToolResultPart;
    }
  | { kind: "block_tool_call"; blockIndex: number; original: KendrToolCallPart }
  | { kind: "block_json"; blockIndex: number; original: JsonValue };

interface MessageBinding {
  original: AgentMessage;
  id: string;
  role: KendrRole;
  contentWasArray: boolean;
  bindings: PartBinding[];
}

interface EncodedMessages {
  messages: KendrMessage[];
  bindings: MessageBinding[];
}

class ServiceCircuit {
  private retryAfter = 0;
  private failed = false;

  constructor(
    private readonly backoffMs: number,
    private readonly logger: PluginLogger,
  ) {}

  canAttempt(): boolean {
    return Date.now() >= this.retryAfter;
  }

  succeeded(): void {
    if (this.failed) {
      this.logger.info("KendrOptimizer loopback service recovered.");
    }
    this.failed = false;
    this.retryAfter = 0;
  }

  failedAttempt(reason: string): void {
    if (!this.failed || Date.now() >= this.retryAfter) {
      this.logger.warn(
        "KendrOptimizer service unavailable; using the original OpenClaw context. " + reason,
      );
    }
    this.failed = true;
    this.retryAfter = Date.now() + this.backoffMs;
  }
}

class KendrClient {
  private readonly optimizeUrl: string;
  private readonly circuit: ServiceCircuit;

  constructor(
    endpoint: string,
    private readonly timeoutMs: number,
    backoffMs: number,
    logger: PluginLogger,
  ) {
    this.optimizeUrl = normalizeLoopbackOrigin(endpoint) + "/v1/optimize";
    this.circuit = new ServiceCircuit(backoffMs, logger);
  }

  async optimize(request: KendrOptimizeRequest): Promise<KendrOutcome | null> {
    if (!this.circuit.canAttempt()) {
      return null;
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const response = await fetch(this.optimizeUrl, {
        method: "POST",
        headers: {
          accept: "application/json",
          "content-type": "application/json",
        },
        body: JSON.stringify(request),
        signal: controller.signal,
        redirect: "error",
        credentials: "omit",
        cache: "no-store",
      });
      if (!response.ok) {
        throw new Error("HTTP " + response.status);
      }

      const parsed: unknown = await response.json();
      const outcome = parseOutcome(parsed);
      this.circuit.succeeded();
      return outcome;
    } catch (error: unknown) {
      this.circuit.failedAttempt(formatError(error));
      return null;
    } finally {
      clearTimeout(timer);
    }
  }
}

class CommitRegistry {
  private readonly keys = new Set<string>();
  private loaded = false;
  private serial: Promise<void> = Promise.resolve();

  constructor(private readonly filePath: string) {}

  async commit(advancementKey: string): Promise<"committed" | "duplicate"> {
    const digest = createHash("sha256").update(advancementKey).digest("hex");
    let status: "committed" | "duplicate" = "committed";

    const operation = this.serial
      .catch(() => undefined)
      .then(async () => {
        await this.load();
        if (this.keys.has(digest)) {
          status = "duplicate";
          return;
        }
        this.keys.add(digest);
        await this.persist();
      });

    this.serial = operation;
    await operation;
    return status;
  }

  private async load(): Promise<void> {
    if (this.loaded) {
      return;
    }

    try {
      const parsed: unknown = JSON.parse(await readFile(this.filePath, "utf8"));
      if (!isRecord(parsed) || parsed.schema !== 1 || !Array.isArray(parsed.keys)) {
        throw new Error("invalid commit registry");
      }
      for (const key of parsed.keys) {
        if (typeof key !== "string" || !/^[a-f0-9]{64}$/.test(key)) {
          throw new Error("invalid commit key");
        }
        this.keys.add(key);
      }
    } catch (error: unknown) {
      if (!isNodeError(error) || error.code !== "ENOENT") {
        throw error;
      }
    }
    this.loaded = true;
  }

  private async persist(): Promise<void> {
    const directory = dirname(this.filePath);
    await mkdir(directory, { recursive: true, mode: 0o700 });
    const temporary = this.filePath + "." + process.pid + "." + randomUUID() + ".tmp";
    try {
      await writeFile(
        temporary,
        JSON.stringify({ schema: 1, keys: [...this.keys].sort() }) + "\n",
        { encoding: "utf8", mode: 0o600 },
      );
      await rename(temporary, this.filePath);
    } finally {
      await rm(temporary, { force: true }).catch(() => undefined);
    }
  }
}

export default function register(api: OpenClawPluginApi): void {
  api.registerContextEngine(PLUGIN_ID, (factoryContext) => {
    const config = parseConfig(factoryContext.config ?? api.pluginConfig ?? {});
    const stateRoot =
      factoryContext.agentDir ?? factoryContext.workspaceDir ?? process.cwd();
    const commitRegistry = new CommitRegistry(
      join(stateRoot, ".kendr-optimizer", "openclaw-commits-v1.json"),
    );
    const client = new KendrClient(
      config.endpoint,
      config.timeoutMs,
      config.failureBackoffMs,
      api.logger,
    );

    return {
      info: {
        id: PLUGIN_ID,
        name: "KendrOptimizer",
        version: "0.1.3",
        ownsCompaction: false,
        acceptedHostParams: [
          "sessionKey",
          "prompt",
          "runtimeSettings",
          "runtimeContext",
        ],
        transcriptSemantics: {
          currentTurnFence: "before-current-turn-entry-v1",
          turnAdvancementIdempotency: "atomic-idempotent-v1",
        },
      },

      async ingest() {
        return { ingested: false };
      },

      async assemble(params) {
        const originalEstimate = estimateTokens(params.messages);
        let encoded: EncodedMessages;
        try {
          encoded = encodeMessages(params.messages);
        } catch (error: unknown) {
          api.logger.warn(
            "KendrOptimizer skipped an unsupported OpenClaw message shape: " +
              formatError(error),
          );
          return {
            messages: params.messages,
            estimatedTokens: originalEstimate,
            promptAuthority: "assembled",
          };
        }

        const model =
          params.runtimeSettings?.model?.resolved ?? params.model ?? null;
        const contextLimit =
          params.tokenBudget ??
          params.runtimeSettings?.limits?.promptTokenBudget ??
          null;
        const request = makeRequest(
          config,
          params.sessionId,
          model,
          contextLimit,
          encoded.messages,
        );
        const outcome = await client.optimize(request);
        if (outcome === null) {
          return {
            messages: params.messages,
            estimatedTokens: originalEstimate,
            promptAuthority: "assembled",
          };
        }

        try {
          const messages = decodeMessages(outcome.content.messages, encoded.bindings);
          const receiptTokens =
            outcome.receipt.status === "shadow"
              ? outcome.receipt.original.tokens
              : outcome.receipt.optimized.tokens;
          api.logger.debug(
            "KendrOptimizer " +
              outcome.receipt.status +
              "; signed input delta " +
              outcome.receipt.token_delta +
              " tokens.",
          );
          return {
            messages,
            estimatedTokens: positiveTokenEstimate(receiptTokens, messages),
            promptAuthority: "assembled",
          };
        } catch (error: unknown) {
          api.logger.warn(
            "KendrOptimizer returned an incompatible message shape; using original context. " +
              formatError(error),
          );
          return {
            messages: params.messages,
            estimatedTokens: originalEstimate,
            promptAuthority: "assembled",
          };
        }
      },

      async compact() {
        return {
          ok: false,
          compacted: false,
          reason:
            "KendrOptimizer is a transform-only engine and does not own OpenClaw transcript compaction.",
        };
      },

      async commitTurn({ advancementKey }) {
        return { status: await commitRegistry.commit(advancementKey) };
      },
    };
  });
}

function makeRequest(
  config: AdapterConfig,
  sessionId: string,
  model: string | null,
  contextLimit: number | null,
  messages: KendrMessage[],
): KendrOptimizeRequest {
  return {
    schema_version: CONTRACT_VERSION,
    phase: "request",
    request_id: "openclaw-" + randomUUID(),
    session_id: sessionId,
    content: {
      messages,
      tools: [],
      output_contract: null,
      metadata: {
        host: "openclaw",
        adapter_version: "0.1.3",
      },
    },
    target: {
      tokenizer_profile: config.tokenizerProfile,
      model,
      context_limit: contextLimit,
      pricing: null,
      cache_segments: [],
    },
    host_capabilities: {
      can_narrow_tools: false,
      can_restore_references: false,
      can_retry_with_full_tools: false,
      streaming_output: true,
    },
    policy: {
      risk_ceiling: config.riskCeiling,
      min_gain_tokens: config.minGainTokens,
      min_gain_percent: config.minGainPercent,
      latency_budget_ms: Math.max(1, config.timeoutMs - 5),
      preserve_cache_prefix: true,
      shadow: config.shadow,
      preserve_recent_messages: config.preserveRecentMessages,
      max_tool_result_chars: config.maxToolResultChars,
      enable_tool_selection: false,
      enable_lossy_tool_output: false,
      enabled_engines: [],
    },
  };
}

function encodeMessages(input: AgentMessage[]): EncodedMessages {
  const messages: KendrMessage[] = [];
  const bindings: MessageBinding[] = [];

  input.forEach((original, messageIndex) => {
    if (!isRecord(original) || typeof original.role !== "string") {
      throw new Error("message " + messageIndex + " has no string role");
    }
    const role = mapRole(original.role);
    const id = "openclaw-message-" + messageIndex;
    const parts: KendrPart[] = [];
    const partBindings: PartBinding[] = [];
    const content = original.content;

    if (typeof content === "string") {
      if (role === "tool" && toolCallId(original) !== null) {
        const callId = toolCallId(original);
        if (callId === null) {
          throw new Error("unreachable missing tool call id");
        }
        const name = optionalString(original.toolName ?? original.name);
        const part: KendrToolResultPart = {
          type: "tool_result",
          call_id: callId,
          content,
          is_error: original.isError === true,
        };
        if (name !== undefined) {
          part.name = name;
        }
        parts.push(part);
        partBindings.push({
          kind: "content_string",
          original: { ...part },
        });
      } else {
        const part: KendrTextPart = { type: "text", text: content };
        parts.push(part);
        partBindings.push({
          kind: "content_string",
          original: { ...part },
        });
      }
    } else if (Array.isArray(content)) {
      content.forEach((block, blockIndex) => {
        if (!isRecord(block)) {
          const value = toJsonValue(block);
          parts.push({ type: "json", value });
          partBindings.push({
            kind: "block_json",
            blockIndex,
            original: toJsonValue(value),
          });
          return;
        }

        if (block.type === "text" && typeof block.text === "string") {
          const callId = role === "tool" ? toolCallId(original) : null;
          if (callId !== null) {
            const name = optionalString(original.toolName ?? original.name);
            const part: KendrToolResultPart = {
              type: "tool_result",
              call_id: callId,
              content: block.text,
              is_error: original.isError === true,
            };
            if (name !== undefined) {
              part.name = name;
            }
            parts.push(part);
            partBindings.push({
              kind: "block_tool_result",
              blockIndex,
              original: { ...part },
            });
          } else {
            parts.push({ type: "text", text: block.text });
            partBindings.push({ kind: "block_text", blockIndex });
          }
          return;
        }

        const call = parseToolCall(block);
        if (call !== null) {
          parts.push(call);
          partBindings.push({
            kind: "block_tool_call",
            blockIndex,
            original: {
              ...call,
              arguments: toJsonValue(call.arguments),
            },
          });
          return;
        }

        const value = toJsonValue(block);
        parts.push({ type: "json", value });
        partBindings.push({
          kind: "block_json",
          blockIndex,
          original: toJsonValue(value),
        });
      });
    } else {
      throw new Error(
        "message " + messageIndex + " content is neither a string nor an array",
      );
    }

    messages.push({
      id,
      role,
      parts,
      metadata: {},
    });
    bindings.push({
      original,
      id,
      role,
      contentWasArray: Array.isArray(content),
      bindings: partBindings,
    });
  });

  return { messages, bindings };
}

function decodeMessages(
  optimized: KendrMessage[],
  bindings: MessageBinding[],
): AgentMessage[] {
  if (optimized.length !== bindings.length) {
    throw new Error("message count changed");
  }

  return optimized.map((message, messageIndex) => {
    const binding = bindings[messageIndex];
    if (binding === undefined) {
      throw new Error("missing message binding");
    }
    if (message.id !== binding.id || message.role !== binding.role) {
      throw new Error("message identity or role changed");
    }
    if (message.parts.length !== binding.bindings.length) {
      throw new Error("message part count changed");
    }

    if (!binding.contentWasArray) {
      const part = message.parts[0];
      if (part === undefined) {
        throw new Error("string content has no returned part");
      }
      const partBinding = binding.bindings[0];
      if (partBinding === undefined || partBinding.kind !== "content_string") {
        throw new Error("string content has an invalid binding");
      }
      const text =
        partBinding.original.type === "text"
          ? returnedTextPart(part)
          : returnedToolResult(part, partBinding.original);
      if (text === binding.original.content) {
        return binding.original;
      }
      return { ...binding.original, content: text };
    }

    const originalBlocks = binding.original.content;
    if (!Array.isArray(originalBlocks)) {
      throw new Error("array binding no longer has array content");
    }
    const blocks = [...originalBlocks];
    let changed = false;

    binding.bindings.forEach((partBinding, partIndex) => {
      const part = message.parts[partIndex];
      if (part === undefined) {
        throw new Error("missing returned part");
      }
      if (partBinding.kind === "content_string") {
        throw new Error("invalid string binding inside an array");
      }
      const originalBlock = blocks[partBinding.blockIndex];

      switch (partBinding.kind) {
        case "block_text": {
          if (!isRecord(originalBlock)) {
            throw new Error("text block changed shape");
          }
          const text = returnedTextPart(part);
          if (text !== originalBlock.text) {
            blocks[partBinding.blockIndex] = { ...originalBlock, text };
            changed = true;
          }
          break;
        }
        case "block_tool_result": {
          if (!isRecord(originalBlock)) {
            throw new Error("tool-result block changed shape");
          }
          const text = returnedToolResult(part, partBinding.original);
          if (text !== originalBlock.text) {
            blocks[partBinding.blockIndex] = { ...originalBlock, text };
            changed = true;
          }
          break;
        }
        case "block_tool_call":
          if (!sameJson(part, partBinding.original)) {
            throw new Error("tool call changed");
          }
          break;
        case "block_json":
          if (
            part.type !== "json" ||
            !sameJson(part.value, partBinding.original)
          ) {
            throw new Error("opaque content block changed");
          }
          break;
      }
    });

    return changed ? { ...binding.original, content: blocks } : binding.original;
  });
}

function parseToolCall(block: UnknownRecord): KendrToolCallPart | null {
  if (
    block.type !== "toolCall" &&
    block.type !== "tool_call" &&
    block.type !== "tool_use"
  ) {
    return null;
  }
  if (typeof block.id !== "string" || typeof block.name !== "string") {
    return null;
  }
  const rawArguments = block.arguments ?? block.input ?? {};
  return {
    type: "tool_call",
    id: block.id,
    name: block.name,
    arguments: toJsonValue(rawArguments),
  };
}

function returnedTextPart(part: KendrPart): string {
  if (part.type !== "text") {
    throw new Error("text part changed type");
  }
  return part.text;
}

function returnedToolResult(
  part: KendrPart,
  original: KendrToolResultPart,
): string {
  if (part.type !== "tool_result") {
    throw new Error("tool-result part changed type");
  }
  if (
    part.call_id !== original.call_id ||
    part.name !== original.name ||
    part.is_error !== original.is_error
  ) {
    throw new Error("tool-result identity changed");
  }
  return part.content;
}

function mapRole(role: string): KendrRole {
  switch (role) {
    case "system":
    case "developer":
    case "user":
    case "assistant":
    case "tool":
      return role;
    case "toolResult":
    case "tool_result":
      return "tool";
    default:
      throw new Error("unsupported role " + role);
  }
}

function toolCallId(message: UnknownRecord): string | null {
  const value = message.toolCallId ?? message.tool_call_id;
  return typeof value === "string" && value.length > 0 ? value : null;
}

function parseOutcome(value: unknown): KendrOutcome {
  if (
    !isRecord(value) ||
    !isRecord(value.content) ||
    !Array.isArray(value.content.messages) ||
    !isRecord(value.receipt)
  ) {
    throw new Error("invalid optimize response");
  }

  const status = value.receipt.status;
  if (
    status !== "applied" &&
    status !== "skipped" &&
    status !== "shadow" &&
    status !== "reverted"
  ) {
    throw new Error("invalid receipt status");
  }
  if (
    !isRecord(value.receipt.original) ||
    !isRecord(value.receipt.optimized) ||
    !isNonNegativeFinite(value.receipt.original.tokens) ||
    !isNonNegativeFinite(value.receipt.optimized.tokens) ||
    !isFiniteNumber(value.receipt.token_delta)
  ) {
    throw new Error("invalid receipt measurements");
  }

  const messages = value.content.messages.map(parseKendrMessage);
  return {
    content: { messages },
    receipt: {
      status,
      original: { tokens: value.receipt.original.tokens },
      optimized: { tokens: value.receipt.optimized.tokens },
      token_delta: value.receipt.token_delta,
    },
  };
}

function parseKendrMessage(value: unknown): KendrMessage {
  if (
    !isRecord(value) ||
    typeof value.id !== "string" ||
    !isKendrRole(value.role) ||
    !Array.isArray(value.parts)
  ) {
    throw new Error("invalid optimized message");
  }
  return {
    id: value.id,
    role: value.role,
    parts: value.parts.map(parseKendrPart),
    metadata: {},
  };
}

function parseKendrPart(value: unknown): KendrPart {
  if (!isRecord(value) || typeof value.type !== "string") {
    throw new Error("invalid optimized content part");
  }
  switch (value.type) {
    case "text":
      if (typeof value.text !== "string") {
        throw new Error("invalid optimized text part");
      }
      return { type: "text", text: value.text };
    case "tool_result": {
      if (
        typeof value.call_id !== "string" ||
        typeof value.content !== "string" ||
        typeof value.is_error !== "boolean"
      ) {
        throw new Error("invalid optimized tool-result part");
      }
      const part: KendrToolResultPart = {
        type: "tool_result",
        call_id: value.call_id,
        content: value.content,
        is_error: value.is_error,
      };
      if (typeof value.name === "string") {
        part.name = value.name;
      }
      return part;
    }
    case "tool_call":
      if (
        typeof value.id !== "string" ||
        typeof value.name !== "string"
      ) {
        throw new Error("invalid optimized tool-call part");
      }
      return {
        type: "tool_call",
        id: value.id,
        name: value.name,
        arguments: toJsonValue(value.arguments),
      };
    case "json":
      return { type: "json", value: toJsonValue(value.value) };
    default:
      throw new Error("unsupported optimized part type");
  }
}

function parseConfig(input: UnknownRecord): AdapterConfig {
  const endpoint = readString(input, "endpoint", DEFAULT_ENDPOINT);
  return {
    endpoint: normalizeLoopbackOrigin(endpoint),
    timeoutMs: readInteger(input, "timeoutMs", 100, 10, 5000),
    failureBackoffMs: readInteger(
      input,
      "failureBackoffMs",
      5000,
      0,
      60000,
    ),
    tokenizerProfile: readEnum(
      input,
      "tokenizerProfile",
      ["approximate", "cl100k_base", "o200k_base"] as const,
      "approximate",
    ),
    riskCeiling: readEnum(
      input,
      "riskCeiling",
      ["pass_through", "representation_safe"] as const,
      "representation_safe",
    ),
    minGainTokens: readInteger(input, "minGainTokens", 8, 0, 1_000_000),
    minGainPercent: readNumber(input, "minGainPercent", 1, 0, 100),
    preserveRecentMessages: readInteger(
      input,
      "preserveRecentMessages",
      6,
      0,
      1000,
    ),
    maxToolResultChars: readInteger(
      input,
      "maxToolResultChars",
      24_000,
      256,
      10_000_000,
    ),
    shadow: readBoolean(input, "shadow", false),
  };
}

function normalizeLoopbackOrigin(input: string): string {
  let url: URL;
  try {
    url = new URL(input);
  } catch {
    throw new Error("endpoint must be an absolute loopback URL");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("endpoint protocol must be HTTP or HTTPS");
  }
  const hostname = url.hostname.toLowerCase();
  if (
    hostname !== "127.0.0.1" &&
    hostname !== "[::1]"
  ) {
    throw new Error(
      "endpoint must use the numeric loopback address 127.0.0.1 or [::1]",
    );
  }
  if (
    url.username !== "" ||
    url.password !== "" ||
    (url.pathname !== "" && url.pathname !== "/") ||
    url.search !== "" ||
    url.hash !== ""
  ) {
    throw new Error("endpoint must be a credential-free loopback origin");
  }
  return url.origin;
}

function estimateTokens(messages: AgentMessage[]): number {
  try {
    return Math.max(1, Math.ceil(JSON.stringify(messages).length / 4));
  } catch {
    return 1;
  }
}

function positiveTokenEstimate(
  receiptTokens: number,
  messages: AgentMessage[],
): number {
  return isNonNegativeFinite(receiptTokens)
    ? Math.max(1, Math.ceil(receiptTokens))
    : estimateTokens(messages);
}

function toJsonValue(value: unknown): JsonValue {
  const serialized = JSON.stringify(value);
  if (serialized === undefined) {
    throw new Error("value is not JSON-compatible");
  }
  return JSON.parse(serialized) as JsonValue;
}

function sameJson(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function isKendrRole(value: unknown): value is KendrRole {
  return (
    value === "system" ||
    value === "developer" ||
    value === "user" ||
    value === "assistant" ||
    value === "tool"
  );
}

function isRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNonNegativeFinite(value: unknown): value is number {
  return isFiniteNumber(value) && value >= 0;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function readString(
  input: UnknownRecord,
  key: string,
  fallback: string,
): string {
  const value = input[key];
  if (value === undefined) {
    return fallback;
  }
  if (typeof value !== "string") {
    throw new Error(key + " must be a string");
  }
  return value;
}

function readInteger(
  input: UnknownRecord,
  key: string,
  fallback: number,
  minimum: number,
  maximum: number,
): number {
  const value = input[key];
  if (value === undefined) {
    return fallback;
  }
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < minimum ||
    value > maximum
  ) {
    throw new Error(
      key + " must be an integer between " + minimum + " and " + maximum,
    );
  }
  return value;
}

function readNumber(
  input: UnknownRecord,
  key: string,
  fallback: number,
  minimum: number,
  maximum: number,
): number {
  const value = input[key];
  if (value === undefined) {
    return fallback;
  }
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value < minimum ||
    value > maximum
  ) {
    throw new Error(
      key + " must be a number between " + minimum + " and " + maximum,
    );
  }
  return value;
}

function readBoolean(
  input: UnknownRecord,
  key: string,
  fallback: boolean,
): boolean {
  const value = input[key];
  if (value === undefined) {
    return fallback;
  }
  if (typeof value !== "boolean") {
    throw new Error(key + " must be a boolean");
  }
  return value;
}

function readEnum<const T extends readonly string[]>(
  input: UnknownRecord,
  key: string,
  allowed: T,
  fallback: T[number],
): T[number] {
  const value = input[key];
  if (value === undefined) {
    return fallback;
  }
  if (
    typeof value !== "string" ||
    !(allowed as readonly string[]).includes(value)
  ) {
    throw new Error(key + " must be one of " + allowed.join(", "));
  }
  return value;
}

function formatError(error: unknown): string {
  if (error instanceof DOMException && error.name === "AbortError") {
    return "request timed out";
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "unknown error";
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

/** Internal test seam; not part of the adapter compatibility contract. */
export const __testing = {
  decodeMessages,
  encodeMessages,
  normalizeLoopbackOrigin,
  parseConfig,
};
