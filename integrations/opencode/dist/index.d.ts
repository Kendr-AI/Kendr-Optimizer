import type { Plugin } from "@opencode-ai/plugin";
export declare const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export declare const OPENCODE_AUDIT: Readonly<{
    package: "@opencode-ai/plugin";
    version: "1.18.15";
    commit: "d7b115f623760e68a4749d16508a9eca350f246f";
    api: "v1";
}>;
export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;
export interface OpenCodeOptimizerOptions {
    coreEndpoint?: string;
    timeoutMs?: number;
    shadow?: boolean;
    experimentalHistory?: boolean;
    experimentalSystem?: boolean;
    fetch?: FetchLike;
}
export declare class LoopbackEndpointError extends Error {
    constructor(message: string);
}
export declare function validateNumericLoopbackEndpoint(raw: string): URL;
export declare class KendrOpenCodeAdapter {
    #private;
    constructor(options?: OpenCodeOptimizerOptions);
    optimizeCurrentMessage(parts: unknown[], sessionId: string, messageId: string | undefined, model: {
        providerID: string;
        modelID: string;
    } | undefined): Promise<unknown[] | undefined>;
    optimizeToolOutput(output: string, input: {
        tool: string;
        sessionID: string;
        callID: string;
    }): Promise<string | undefined>;
    optimizeHistory(messages: unknown[]): Promise<unknown[] | undefined>;
    optimizeSystem(system: string[], sessionId: string | undefined, model: unknown): Promise<string[] | undefined>;
}
export declare function createOpenCodePlugin(options?: OpenCodeOptimizerOptions): Plugin;
export declare const KendrOptimizerPlugin: Plugin;
