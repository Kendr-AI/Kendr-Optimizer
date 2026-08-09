import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { afterEach, describe, test } from 'node:test';

import { optimizeNanoClawPrompt, resetKendrOptimizerForTests } from './kendr-optimizer.js';

afterEach(() => {
  resetKendrOptimizerForTests();
  delete process.env.KENDR_OPTIMIZER_ENDPOINT;
  delete process.env.KENDR_OPTIMIZER_TIMEOUT_MS;
  delete process.env.KENDR_OPTIMIZER_BACKOFF_MS;
});

function responseFor(request: any, text: string, mutate?: (outcome: any) => void) {
  const message = structuredClone(request.content.messages[0]);
  message.parts[0].text = text;
  const outcome = {
    content: {
      messages: [message],
      tools: [],
      output_contract: null,
      metadata: request.content.metadata,
    },
    receipt: {
      status: 'applied',
      original: { tokens: 100 },
      optimized: { tokens: 60 },
      token_delta: 40,
    },
  };
  mutate?.(outcome);
  return { statusCode: 200, body: JSON.stringify(outcome) };
}

describe('optimizeNanoClawPrompt', () => {
  test('sends one bounded prompt envelope and accepts a validated text change', async () => {
    let seenUrl = '';
    let seenRequest: any;
    let seenTimeout = 0;
    const transport = async (url: string, body: string, timeoutMs: number) => {
      seenUrl = url;
      seenRequest = JSON.parse(body);
      seenTimeout = timeoutMs;
      return responseFor(seenRequest, 'compact prompt');
    };

    const value = await optimizeNanoClawPrompt('redundant\n\n\n\nprompt', { transport });
    assert.equal(value, 'compact prompt');
    assert.equal(seenUrl, 'http://127.0.0.1:7331/v1/optimize');
    assert.equal(seenTimeout, 40);
    assert.equal(seenRequest.phase, 'request');
    assert.equal(seenRequest.content.messages.length, 1);
    assert.deepEqual(seenRequest.content.tools, []);
    assert.equal(seenRequest.host_capabilities.can_narrow_tools, false);
    assert.equal(seenRequest.policy.risk_ceiling, 'representation_safe');
    assert.equal(seenRequest.policy.enable_generation_policy, false);
  });

  test('skips native slash commands without a sidecar call', async () => {
    let calls = 0;
    const transport = async () => {
      calls += 1;
      return { statusCode: 200, body: '{}' };
    };
    assert.equal(await optimizeNanoClawPrompt('  /compact', { transport }), '  /compact');
    assert.equal(calls, 0);
  });

  test('rejects non-loopback endpoints before transport', async () => {
    let calls = 0;
    const transport = async () => {
      calls += 1;
      return { statusCode: 200, body: '{}' };
    };
    assert.equal(
      await optimizeNanoClawPrompt('original', {
        endpoint: 'https://optimizer.example/v1',
        transport,
      }),
      'original',
    );
    assert.equal(calls, 0);
  });

  test('uses a direct loopback HTTP transport', async () => {
    const server = createServer((request, response) => {
      const chunks: Buffer[] = [];
      request.on('data', (chunk: Buffer) => chunks.push(chunk));
      request.on('end', () => {
        const envelope = JSON.parse(Buffer.concat(chunks).toString('utf8'));
        const result = responseFor(envelope, 'direct compact');
        response.writeHead(result.statusCode, { 'content-type': 'application/json' });
        response.end(result.body);
      });
    });
    const endpoint = await listen(server);
    try {
      assert.equal(
        await optimizeNanoClawPrompt('direct original', { endpoint, timeoutMs: 250 }),
        'direct compact',
      );
    } finally {
      await close(server);
    }
  });

  test('does not follow a loopback redirect', async () => {
    let redirectTargetCalls = 0;
    const target = createServer((_request, response) => {
      redirectTargetCalls += 1;
      response.end('{}');
    });
    const targetEndpoint = await listen(target);
    const redirect = createServer((_request, response) => {
      response.writeHead(302, { location: `${targetEndpoint}/capture` });
      response.end();
    });
    const endpoint = await listen(redirect);
    try {
      assert.equal(
        await optimizeNanoClawPrompt('do not redirect', { endpoint, timeoutMs: 250 }),
        'do not redirect',
      );
      assert.equal(redirectTargetCalls, 0);
    } finally {
      await close(redirect);
      await close(target);
    }
  });

  test('fails open on a changed message identity', async () => {
    const transport = async (_url: string, body: string) => {
      const request = JSON.parse(body);
      return responseFor(request, 'malicious', (outcome) => {
        outcome.content.messages[0].id = 'different';
      });
    };
    assert.equal(await optimizeNanoClawPrompt('original', { transport }), 'original');
  });

  test('opens a circuit after failure and never throws', async () => {
    let calls = 0;
    const transport = async () => {
      calls += 1;
      throw new Error('offline');
    };
    assert.equal(await optimizeNanoClawPrompt('first', { transport, now: () => 1_000 }), 'first');
    assert.equal(await optimizeNanoClawPrompt('second', { transport, now: () => 1_001 }), 'second');
    assert.equal(calls, 1);
  });
});

async function listen(server: Server): Promise<string> {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve());
  });
  const address = server.address() as AddressInfo;
  return `http://127.0.0.1:${address.port}`;
}

async function close(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

describe('poll-loop installation', () => {
  test('the initial and follow-up prompt seams are wired', () => {
    const source = readFileSync(new URL('./poll-loop.ts', import.meta.url), 'utf8');
    assert.ok(source.includes("import { optimizeNanoClawPrompt } from './kendr-optimizer.js';"));
    assert.ok(source.includes('prompt: await optimizeNanoClawPrompt(prompt),'));
    assert.ok(source.includes('const optimizedPrompt = await optimizeNanoClawPrompt(prompt);'));
    assert.match(source, /if \(done\) return;\r?\n\s+query\.push\(optimizedPrompt\);/);
  });
});
