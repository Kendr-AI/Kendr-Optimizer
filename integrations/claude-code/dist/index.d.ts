import { type Server } from "node:http";
export declare const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export declare const DEFAULT_BRIDGE_HOST = "127.0.0.1";
export declare const DEFAULT_BRIDGE_PORT = 7332;
export declare const CLAUDE_CODE_AUDIT: Readonly<{
    version: "2.1.224";
    commit: "66edf5358349356774812264b75b8ea792f0d0a3";
}>;
export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;
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
export declare class LoopbackEndpointError extends Error {
    constructor(message: string);
}
export declare function validateNumericLoopbackEndpoint(raw: string): URL;
export declare function handleClaudeCodeHook(pathname: string, payload: ClaudeHookPayload, options?: ClaudeCodeBridgeOptions): Promise<JsonRecord>;
export declare function createClaudeCodeBridgeServer(options?: ClaudeCodeBridgeOptions): Server;
export {};
