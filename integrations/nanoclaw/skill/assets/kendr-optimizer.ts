/**
 * Fail-open prompt-string adapter for NanoClaw's generic provider seam.
 *
 * NanoClaw's AgentProvider contract exposes a formatted prompt string, not the
 * provider's history, tool schemas, or tool results. This module intentionally
 * makes no claim beyond that string boundary.
 */

import { randomUUID } from 'node:crypto';
import { request as httpRequest } from 'node:http';

const CONTRACT_VERSION = 'kendr.optimize/v1';
const DEFAULT_ENDPOINT = 'http://127.0.0.1:7331';
const DEFAULT_TIMEOUT_MS = 40;
const DEFAULT_BACKOFF_MS = 30_000;
const MAX_BODY_BYTES = 32 * 1024 * 1024;

let retryAfter = 0;
let failureLogged = false;

interface KendrOutcome {
  content: {
    messages: Array<{
      id: string;
      role: string;
      parts: Array<Record<string, unknown>>;
    }>;
    tools: unknown[];
    output_contract: unknown;
  };
  receipt: {
    status: string;
    original: { tokens: number };
    optimized: { tokens: number };
    token_delta: number;
  };
}

export interface PromptOptimizerOptions {
  endpoint?: string;
  timeoutMs?: number;
  backoffMs?: number;
  transport?: LoopbackTransport;
  now?: () => number;
}

export interface LoopbackResponse {
  statusCode: number;
  body: string;
}

export type LoopbackTransport = (
  url: string,
  body: string,
  timeoutMs: number,
) => Promise<LoopbackResponse>;

/** Optimize one newly formatted inbound prompt. Any uncertainty returns input. */
export async function optimizeNanoClawPrompt(
  prompt: string,
  options: PromptOptimizerOptions = {},
): Promise<string> {
  if (!prompt || prompt.trimStart().startsWith('/')) return prompt;

  const now = options.now ?? Date.now;
  if (now() < retryAfter) return prompt;

  let endpoint: string;
  let timeoutMs: number;
  let backoffMs: number;
  try {
    endpoint = loopbackOrigin(options.endpoint ?? process.env.KENDR_OPTIMIZER_ENDPOINT ?? DEFAULT_ENDPOINT);
    timeoutMs = boundedInteger(
      options.timeoutMs ?? envInteger('KENDR_OPTIMIZER_TIMEOUT_MS') ?? DEFAULT_TIMEOUT_MS,
      5,
      250,
    );
    backoffMs = boundedInteger(
      options.backoffMs ?? envInteger('KENDR_OPTIMIZER_BACKOFF_MS') ?? DEFAULT_BACKOFF_MS,
      100,
      300_000,
    );
  } catch {
    logFailureOnce('configuration');
    return prompt;
  }

  const messageId = `nanoclaw-prompt-${randomUUID()}`;
  const request = {
    schema_version: CONTRACT_VERSION,
    phase: 'request',
    request_id: `nanoclaw-${randomUUID()}`,
    session_id: null,
    content: {
      messages: [
        {
          id: messageId,
          role: 'user',
          parent_id: null,
          turn_id: null,
          parts: [{ type: 'text', text: prompt }],
          metadata: {},
        },
      ],
      tools: [],
      output_contract: null,
      metadata: { host: 'nanoclaw', adapter_version: '0.1.0', surface: 'inbound_prompt_string' },
    },
    target: {
      tokenizer_profile: tokenizerProfile(),
      model: null,
      context_limit: null,
      pricing: null,
      cache_segments: [],
    },
    generation: {},
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
      risk_ceiling: 'representation_safe',
      min_gain_tokens: 8,
      min_gain_percent: 1,
      latency_budget_ms: Math.max(1, timeoutMs - 5),
      preserve_cache_prefix: true,
      shadow: false,
      preserve_recent_messages: 1,
      max_tool_result_chars: 24_000,
      enable_tool_selection: false,
      enable_lossy_tool_output: false,
      enable_generation_policy: false,
      min_expected_output_saving_tokens: 32,
      enabled_engines: [],
    },
  };

  const body = JSON.stringify(request);
  if (Buffer.byteLength(body, 'utf8') > MAX_BODY_BYTES) return prompt;
  try {
    // node:http connects directly to the validated literal loopback socket. It
    // neither consults HTTP(S)_PROXY nor follows redirects.
    const response = await (options.transport ?? postLoopbackJson)(
      `${endpoint}/v1/optimize`,
      body,
      timeoutMs,
    );
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw new Error('non-success status');
    }
    if (Buffer.byteLength(response.body, 'utf8') > MAX_BODY_BYTES) {
      throw new Error('response too large');
    }
    const outcome: unknown = JSON.parse(response.body);
    const optimized = parseOutcome(outcome, messageId);
    retryAfter = 0;
    failureLogged = false;
    return optimized ?? prompt;
  } catch {
    retryAfter = now() + backoffMs;
    logFailureOnce('sidecar');
    return prompt;
  }
}

async function postLoopbackJson(url: string, body: string, timeoutMs: number): Promise<LoopbackResponse> {
  return new Promise((resolve, reject) => {
    let timer: NodeJS.Timeout;
    const fail = (error: Error) => {
      clearTimeout(timer);
      reject(error);
    };
    const succeed = (response: LoopbackResponse) => {
      clearTimeout(timer);
      resolve(response);
    };
    const request = httpRequest(
      url,
      {
        method: 'POST',
        agent: false,
        headers: {
          'content-type': 'application/json',
          accept: 'application/json',
          'content-length': String(Buffer.byteLength(body, 'utf8')),
          'user-agent': 'kendr-optimizer-nanoclaw/0.1.0',
        },
      },
      (response) => {
        response.on('error', fail);
        const declared = Number(response.headers['content-length'] ?? '0');
        if (Number.isFinite(declared) && declared > MAX_BODY_BYTES) {
          fail(new Error('response too large'));
          response.destroy(new Error('response too large'));
          return;
        }
        const chunks: Buffer[] = [];
        let bytes = 0;
        response.on('data', (chunk: Buffer | string) => {
          const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
          bytes += buffer.length;
          if (bytes > MAX_BODY_BYTES) {
            fail(new Error('response too large'));
            response.destroy(new Error('response too large'));
            return;
          }
          chunks.push(buffer);
        });
        response.on('end', () => {
          succeed({
            statusCode: response.statusCode ?? 0,
            body: Buffer.concat(chunks, bytes).toString('utf8'),
          });
        });
      },
    );
    timer = setTimeout(() => request.destroy(new Error('deadline exceeded')), timeoutMs);
    timer.unref?.();
    request.on('error', fail);
    request.end(body);
  });
}

function parseOutcome(value: unknown, messageId: string): string | null {
  if (!isRecord(value) || !isRecord(value.content) || !isRecord(value.receipt)) return null;
  const outcome = value as unknown as KendrOutcome;
  if (outcome.receipt.status !== 'applied') return null;
  if (
    !isRecord(outcome.receipt.original) ||
    !isRecord(outcome.receipt.optimized) ||
    !isNonNegativeNumber(outcome.receipt.original.tokens) ||
    !isNonNegativeNumber(outcome.receipt.optimized.tokens) ||
    !isFiniteNumber(outcome.receipt.token_delta)
  ) {
    return null;
  }
  if (
    !Array.isArray(outcome.content.messages) ||
    outcome.content.messages.length !== 1 ||
    !Array.isArray(outcome.content.tools) ||
    outcome.content.tools.length !== 0 ||
    outcome.content.output_contract !== null
  ) {
    return null;
  }
  const message = outcome.content.messages[0];
  if (
    !isRecord(message) ||
    message.id !== messageId ||
    message.role !== 'user' ||
    !Array.isArray(message.parts) ||
    message.parts.length !== 1
  ) {
    return null;
  }
  const part = message.parts[0];
  if (!isRecord(part) || part.type !== 'text' || typeof part.text !== 'string') return null;
  return part.text;
}

function loopbackOrigin(raw: string): string {
  const parsed = new URL(raw);
  if (
    parsed.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(parsed.hostname) ||
    parsed.username ||
    parsed.password ||
    !parsed.port ||
    !Number.isInteger(Number(parsed.port)) ||
    Number(parsed.port) < 1 ||
    Number(parsed.port) > 65_535 ||
    parsed.pathname !== '/' ||
    parsed.search ||
    parsed.hash
  ) {
    throw new Error('endpoint is not a literal loopback HTTP origin');
  }
  return parsed.origin;
}

function tokenizerProfile(): 'approximate' | 'cl100k_base' | 'o200k_base' {
  const value = process.env.KENDR_OPTIMIZER_TOKENIZER ?? 'o200k_base';
  return value === 'approximate' || value === 'cl100k_base' || value === 'o200k_base'
    ? value
    : 'o200k_base';
}

function envInteger(name: string): number | undefined {
  const value = process.env[name];
  if (value === undefined) return undefined;
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) throw new Error(`${name} is not an integer`);
  return parsed;
}

function boundedInteger(value: number, minimum: number, maximum: number): number {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error('integer is outside the supported range');
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isNonNegativeNumber(value: unknown): value is number {
  return isFiniteNumber(value) && value >= 0;
}

function logFailureOnce(kind: 'configuration' | 'sidecar'): void {
  if (failureLogged) return;
  failureLogged = true;
  console.error(`[kendr-optimizer] ${kind} unavailable; passing the original prompt through`);
}

/** @internal Test-only state reset; no prompt data is retained. */
export function resetKendrOptimizerForTests(): void {
  retryAfter = 0;
  failureLogged = false;
}
