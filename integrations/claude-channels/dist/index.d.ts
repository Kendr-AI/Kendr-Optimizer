export declare const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export declare const CLAUDE_CHANNELS_AUDIT: Readonly<{
    version: "2.1.224";
    commit: "66edf5358349356774812264b75b8ea792f0d0a3";
    status: "research_preview";
}>;
export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;
export interface ChannelNotification<Meta = unknown> {
    content: string;
    meta?: Meta;
    [key: string]: unknown;
}
export interface ChannelMessageContext {
    senderAuthorized: boolean;
    sessionId?: string;
    channelName?: string;
    senderClass?: string;
}
export interface ClaudeChannelOptimizerOptions {
    coreEndpoint?: string;
    timeoutMs?: number;
    shadow?: boolean;
    fetch?: FetchLike;
}
export type ChannelOptimizationReason = "optimized" | "sender_not_authorized" | "empty_content" | "shadow_only" | "optimizer_unavailable" | "not_applied_or_invalid" | "unchanged";
export interface ChannelOptimizationResult<T extends ChannelNotification> {
    notification: T;
    applied: boolean;
    reason: ChannelOptimizationReason;
    requestId?: string;
}
export declare class LoopbackEndpointError extends Error {
    constructor(message: string);
}
export declare function validateNumericLoopbackEndpoint(raw: string): URL;
export declare class KendrClaudeChannelOptimizer {
    #private;
    constructor(options?: ClaudeChannelOptimizerOptions);
    prepareNotification<T extends ChannelNotification>(notification: T, context: ChannelMessageContext): Promise<ChannelOptimizationResult<T>>;
}
export declare function createClaudeChannelOptimizer(options?: ClaudeChannelOptimizerOptions): KendrClaudeChannelOptimizer;
