import type { BeforeAgentStartEvent, ContextEvent, ExtensionAPI, ExtensionContext, MessageEndEvent, ToolResultEvent } from "@earendil-works/pi-coding-agent";
export declare const DEFAULT_CORE_ENDPOINT = "http://127.0.0.1:7331";
export declare const PI_AUDIT: Readonly<{
    package: "@earendil-works/pi-coding-agent";
    version: "0.84.1";
    commit: "53fa77ccd8a279eb87e92294ef3687b03ff80112";
}>;
export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;
export interface PiOptimizerOptions {
    coreEndpoint?: string;
    timeoutMs?: number;
    shadow?: boolean;
    observeOutput?: boolean;
    fetch?: FetchLike;
}
export declare class LoopbackEndpointError extends Error {
    constructor(message: string);
}
export declare function validateNumericLoopbackEndpoint(raw: string): URL;
export declare class KendrPiAdapter {
    #private;
    constructor(options?: PiOptimizerOptions);
    optimizeSystemPrompt(event: BeforeAgentStartEvent, context: ExtensionContext): Promise<string | undefined>;
    optimizeContext(event: ContextEvent, context: ExtensionContext): Promise<ContextEvent["messages"] | undefined>;
    optimizeToolResult(event: ToolResultEvent, context: ExtensionContext): Promise<ToolResultEvent["content"] | undefined>;
    observeAssistantOutput(event: MessageEndEvent, context: ExtensionContext): Promise<void>;
}
export declare function createPiOptimizerExtension(options?: PiOptimizerOptions): (pi: ExtensionAPI) => void;
export default function kendrPiOptimizer(pi: ExtensionAPI): void;
