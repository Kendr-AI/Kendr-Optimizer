import { randomUUID } from "node:crypto";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";

export const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export const DEFAULT_BRIDGE_HOST = "127.0.0.1";
export const DEFAULT_BRIDGE_PORT = 7332;
export const CLAUDE_CODE_AUDIT = Object.freeze({
  version: "2.1.224",
  commit: "66edf5358349356774812264b75b8ea792f0d0a3",
});

export type FetchLike = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

export interface ClaudeCodeBridgeOptions {
  coreEndpoint?: string;
  timeoutMs?: number;
  shadow?: boolean;
  fetch?: FetchLike;
  maxHookBodyBytes?: number;
}

export interface ClaudeHookPayload {
  hook_event_name?: string;
  session_id?: string;
  agent_id?: string;
  agent_type?: string;
  cwd?: string;
  prompt?: string;
  tool_name?: string;
  tool_use_id?: string;
  tool_input?: unknown;
  tool_response?: unknown;
  last_assistant_message?: string;
  [key: string]: unknown;
}

type JsonRecord = Record<string, unknown>;
type PathSegment = string | number;

interface TextBinding {
  path: PathSegment[];
  original: string;
  callId: string;
}

interface CorePart {
  type: string;
  [key: string]: unknown;
}

interface CoreMessage {
  id: string;
  role: string;
  parts: CorePart[];
  [key: string]: unknown;
}

interface CoreOutcome {
  content: {
    messages: CoreMessage[];
    [key: string]: unknown;
  };
  receipt: {
    schema_version: string;
    request_id: string;
    status: string;
    token_delta: number;
    verified_savings: boolean;
    original: { tokens: number };
    optimized: { tokens: number };
    [key: string]: unknown;
  };
  [key: string]: unknown;
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
    throw new LoopbackEndpointError("Kendr endpoint must use plain HTTP on loopback");
  }
  if (hostname !== "127.0.0.1" && hostname !== "[::1]" && hostname !== "::1") {
    throw new LoopbackEndpointError("Kendr endpoint must use numeric loopback, not a hostname");
  }
  if (endpoint.username || endpoint.password) {
    throw new LoopbackEndpointError("Kendr endpoint must not contain credentials");
  }
  if (endpoint.pathname !== "/" || endpoint.search || endpoint.hash) {
    throw new LoopbackEndpointError("Kendr endpoint must not contain a path, query, or fragment");
  }
  return endpoint;
}

class KendrClient {
  readonly #endpoint: URL;
  readonly #timeoutMs: number;
  readonly #fetch: FetchLike;

  constructor(options: ClaudeCodeBridgeOptions = {}) {
    this.#endpoint = validateNumericLoopbackEndpoint(
      options.coreEndpoint ?? DEFAULT_CORE_ENDPOINT,
    );
    this.#timeoutMs = positiveInteger(options.timeoutMs, 100, 10_000);
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  async optimize(request: JsonRecord): Promise<unknown> {
    return this.#post("/v1/optimize", request);
  }

  async analyze(request: JsonRecord): Promise<unknown> {
    return this.#post("/v1/analyze", request);
  }

  async #post(path: string, body: JsonRecord): Promise<unknown> {
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
      if (!response.ok) return undefined;
      return await response.json();
    } catch {
      return undefined;
    } finally {
      clearTimeout(timer);
    }
  }
}

export async function handleClaudeCodeHook(
  pathname: string,
  payload: ClaudeHookPayload,
  options: ClaudeCodeBridgeOptions = {},
): Promise<JsonRecord> {
  try {
    const client = new KendrClient(options);
    if (pathname === "/hooks/claude-code/post-tool-use") {
      return await handlePostToolUse(payload, client, options.shadow === true);
    }
    if (pathname === "/hooks/claude-code/user-prompt-submit") {
      await shadowText(payload.prompt, "user", "request", payload, client);
      return {};
    }
    if (pathname === "/hooks/claude-code/stop") {
      await shadowText(
        payload.last_assistant_message,
        "assistant",
        "output_observation",
        payload,
        client,
      );
      return {};
    }
  } catch {
    // Claude Code HTTP-hook failures are non-blocking, but returning an empty
    // 2xx response also avoids surfacing avoidable hook errors in the UI.
  }
  return {};
}

async function handlePostToolUse(
  payload: ClaudeHookPayload,
  client: KendrClient,
  shadow: boolean,
): Promise<JsonRecord> {
  if (payload.hook_event_name && payload.hook_event_name !== "PostToolUse") return {};
  if (!("tool_response" in payload)) return {};

  const toolUseId = nonEmptyString(payload.tool_use_id) ?? randomUUID();
  const bindings = collectTextBindings(payload.tool_response, toolUseId);
  if (bindings.length === 0) return {};

  const requestId = "claude-code-" + randomUUID();
  const messageId = "tool-result-" + toolUseId;
  const request = buildRequest({
    requestId,
    sessionId: nonEmptyString(payload.session_id),
    phase: "tool_result",
    shadow,
    messages: [
      {
        id: messageId,
        role: "tool",
        parts: bindings.map((binding) => ({
          type: "tool_result",
          call_id: binding.callId,
          name: nonEmptyString(payload.tool_name),
          content: binding.original,
          is_error: false,
        })),
        metadata: {
          host: "claude-code",
          audited_host_version: CLAUDE_CODE_AUDIT.version,
        },
      },
    ],
  });

  const rawOutcome = shadow ? await client.analyze(request) : await client.optimize(request);
  if (shadow) return {};
  const replacement = decodeToolResultOutcome(rawOutcome, requestId, messageId, bindings);
  if (!replacement) return {};

  const updatedToolOutput = applyTextBindings(payload.tool_response, bindings, replacement);
  if (updatedToolOutput === undefined || deepEqual(updatedToolOutput, payload.tool_response)) {
    return {};
  }
  return {
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      updatedToolOutput,
    },
  };
}

async function shadowText(
  text: unknown,
  role: "user" | "assistant",
  phase: "request" | "output_observation",
  payload: ClaudeHookPayload,
  client: KendrClient,
): Promise<void> {
  if (typeof text !== "string" || text.length === 0) return;
  const requestId = "claude-code-shadow-" + randomUUID();
  await client.analyze(
    buildRequest({
      requestId,
      sessionId: nonEmptyString(payload.session_id),
      phase,
      shadow: true,
      messages: [
        {
          id: role + "-" + requestId,
          role,
          parts: [{ type: "text", text }],
          metadata: {
            host: "claude-code",
            audited_host_version: CLAUDE_CODE_AUDIT.version,
            observation_only: true,
          },
        },
      ],
    }),
  );
}

function buildRequest(input: {
  requestId: string;
  sessionId: string | undefined;
  phase: "request" | "tool_result" | "output_observation";
  shadow: boolean;
  messages: CoreMessage[];
}): JsonRecord {
  return {
    schema_version: "kendr.optimize/v1",
    phase: input.phase,
    request_id: input.requestId,
    ...(input.sessionId ? { session_id: input.sessionId } : {}),
    content: {
      messages: input.messages,
      tools: [],
      metadata: {
        adapter: "@kendr/optimizer-claude-code",
        host_version: CLAUDE_CODE_AUDIT.version,
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
      shadow: input.shadow,
      enable_tool_selection: false,
      enable_lossy_tool_output: false,
      enable_generation_policy: false,
    },
  };
}

function collectTextBindings(value: unknown, toolUseId: string): TextBinding[] {
  const bindings: TextBinding[] = [];
  if (typeof value === "string") {
    if (value.length > 0) bindings.push({ path: [], original: value, callId: toolUseId + ":0" });
    return bindings;
  }
  visitText(value, [], undefined, bindings, toolUseId);
  return bindings;
}

function visitText(
  value: unknown,
  path: PathSegment[],
  key: string | undefined,
  bindings: TextBinding[],
  toolUseId: string,
): void {
  if (typeof value === "string") {
    if (value.length > 0 && key !== undefined && isTextBearingKey(key)) {
      bindings.push({
        path,
        original: value,
        callId: toolUseId + ":" + bindings.length,
      });
    }
    return;
  }
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      visitText(value[index], [...path, index], key, bindings, toolUseId);
    }
    return;
  }
  if (!isRecord(value)) return;
  for (const [childKey, child] of Object.entries(value)) {
    visitText(child, [...path, childKey], childKey, bindings, toolUseId);
  }
}

function isTextBearingKey(key: string): boolean {
  return /^(stdout|stderr|output|content|text|result|message|error|body|log|logs|diff|patch|transcript|summary|response)$/i.test(
    key,
  );
}

function decodeToolResultOutcome(
  raw: unknown,
  requestId: string,
  messageId: string,
  bindings: TextBinding[],
): string[] | undefined {
  if (!isCoreOutcome(raw)) return undefined;
  if (raw.receipt.schema_version !== "kendr.receipt/v1") return undefined;
  if (
    raw.receipt.request_id !== requestId ||
    raw.receipt.status !== "applied" ||
    raw.receipt.token_delta <= 0 ||
    raw.receipt.original.tokens <= raw.receipt.optimized.tokens
  ) {
    return undefined;
  }
  if (raw.content.messages.length !== 1) return undefined;
  const message = raw.content.messages[0];
  if (!message || message.id !== messageId || message.role !== "tool") return undefined;
  if (!Array.isArray(message.parts) || message.parts.length !== bindings.length) return undefined;

  const output: string[] = [];
  for (let index = 0; index < bindings.length; index += 1) {
    const part = message.parts[index];
    const binding = bindings[index];
    if (!part || !binding || part.type !== "tool_result") return undefined;
    if (part.call_id !== binding.callId || typeof part.content !== "string") return undefined;
    output.push(part.content);
  }
  return output;
}

function applyTextBindings(
  original: unknown,
  bindings: TextBinding[],
  replacements: string[],
): unknown | undefined {
  if (bindings.length !== replacements.length) return undefined;
  if (bindings.length === 1 && bindings[0]?.path.length === 0) return replacements[0];
  const copy = structuredClone(original);
  for (let index = 0; index < bindings.length; index += 1) {
    const binding = bindings[index];
    const replacement = replacements[index];
    if (!binding || replacement === undefined || !setAtPath(copy, binding.path, replacement)) {
      return undefined;
    }
  }
  return copy;
}

function setAtPath(root: unknown, path: PathSegment[], value: string): boolean {
  if (path.length === 0) return false;
  let cursor: unknown = root;
  for (let index = 0; index < path.length - 1; index += 1) {
    const segment = path[index];
    if (segment === undefined || (!isRecord(cursor) && !Array.isArray(cursor))) return false;
    cursor = (cursor as Record<PathSegment, unknown>)[segment];
  }
  const last = path[path.length - 1];
  if (last === undefined || (!isRecord(cursor) && !Array.isArray(cursor))) return false;
  (cursor as Record<PathSegment, unknown>)[last] = value;
  return true;
}

function isCoreOutcome(value: unknown): value is CoreOutcome {
  if (!isRecord(value) || !isRecord(value.content) || !Array.isArray(value.content.messages)) {
    return false;
  }
  if (
    !isRecord(value.receipt) ||
    !isRecord(value.receipt.original) ||
    !isRecord(value.receipt.optimized)
  ) {
    return false;
  }
  return (
    typeof value.receipt.schema_version === "string" &&
    typeof value.receipt.request_id === "string" &&
    typeof value.receipt.status === "string" &&
    Number.isSafeInteger(value.receipt.token_delta) &&
    typeof value.receipt.verified_savings === "boolean" &&
    Number.isSafeInteger(value.receipt.original.tokens) &&
    Number.isSafeInteger(value.receipt.optimized.tokens)
  );
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function positiveInteger(value: number | undefined, fallback: number, max: number): number {
  return Number.isSafeInteger(value) && value !== undefined && value > 0 && value <= max
    ? value
    : fallback;
}

function deepEqual(left: unknown, right: unknown): boolean {
  try {
    return JSON.stringify(left) === JSON.stringify(right);
  } catch {
    return false;
  }
}

export function createClaudeCodeBridgeServer(
  options: ClaudeCodeBridgeOptions = {},
): Server {
  const maxBodyBytes = positiveInteger(options.maxHookBodyBytes, 1_048_576, 16_777_216);
  return createServer(async (request, response) => {
    await serveHookRequest(request, response, options, maxBodyBytes);
  });
}

async function serveHookRequest(
  request: IncomingMessage,
  response: ServerResponse,
  options: ClaudeCodeBridgeOptions,
  maxBodyBytes: number,
): Promise<void> {
  const pathname = parsePathname(request.url);
  if (!isKnownPath(pathname)) {
    sendJson(response, 404, {});
    return;
  }
  if (request.method !== "POST") {
    sendJson(response, 405, {});
    return;
  }
  try {
    const body = await readJsonBody(request, maxBodyBytes);
    const output = isRecord(body)
      ? await handleClaudeCodeHook(pathname, body as ClaudeHookPayload, options)
      : {};
    sendJson(response, 200, output);
  } catch {
    sendJson(response, 200, {});
  }
}

function parsePathname(raw: string | undefined): string {
  try {
    return new URL(raw ?? "/", "http://127.0.0.1").pathname;
  } catch {
    return "/";
  }
}

function isKnownPath(pathname: string): boolean {
  return (
    pathname === "/hooks/claude-code/post-tool-use" ||
    pathname === "/hooks/claude-code/user-prompt-submit" ||
    pathname === "/hooks/claude-code/stop"
  );
}

function readJsonBody(request: IncomingMessage, maxBodyBytes: number): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    request.on("data", (chunk: Buffer | string) => {
      const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      size += buffer.length;
      if (size > maxBodyBytes) {
        reject(new Error("hook body too large"));
        request.destroy();
        return;
      }
      chunks.push(buffer);
    });
    request.on("end", () => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
    request.on("error", reject);
  });
}

function sendJson(response: ServerResponse, status: number, body: JsonRecord): void {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(payload),
    "cache-control": "no-store",
  });
  response.end(payload);
}
