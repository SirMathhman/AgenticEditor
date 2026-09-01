/// Shared types, constants, and helpers for the chat panel and its
/// sub-components. Kept separate so the presentation components and the
/// orchestrator can share them without importing each other.

import type { Model } from "../../lib/ipc";

/// The streamed output of a subagent spawned via the `spawn_subagent` tool.
/// Accumulated live while the subagent runs and then attached to the
/// `spawn_subagent` tool call so it renders nested inside that call.
export interface SubagentData {
  /// The role the subagent was given (a custom-agent name or free-form text).
  role: string;
  /// The subagent's visible reply text, accumulated from its chunks.
  content: string;
  /// The subagent's chain-of-thought, if it produced any.
  reasoning: string;
  /// The tool calls the subagent made while working.
  tools: ToolCall[];
  /// The subagent's final answer, set once it finishes (null while running).
  result: string | null;
}

/// A tool call the agent made during a turn, shown in the message's
/// tool-calls panel.
export interface ToolCall {
  name: string;
  result: string;
  /// Present only on `spawn_subagent` calls: the subagent's own streamed
  /// output, rendered nested inside the tool call.
  subagent?: SubagentData;
}

export interface ChatMessage {
  role: "user" | "agent" | "system";
  text: string;
  /// The model's chain-of-thought, present only on agent messages from
  /// reasoning models.
  thinking?: string | null;
  /// The tool calls the agent made while producing this message.
  toolCalls?: ToolCall[];
}

/// A provider group in the model picker: the provider label plus its models
/// in their original order.
export interface ModelGroup {
  provider: string;
  models: Model[];
}

/// The maximum length of a model's chain-of-thought kept for display and
/// persistence. Reasoning models can emit very long thinking; capping it keeps
/// session files from growing unbounded. Longer output is truncated with an
/// ellipsis marker.
export const MAX_THINKING_CHARS = 4000;

/// Truncates a chain-of-thought to `MAX_THINKING_CHARS`, appending a marker
/// when it was cut. Returns `null` for empty/whitespace-only input.
export function capThinking(
  thinking: string | null | undefined,
): string | null {
  if (!thinking) {
    return null;
  }
  const trimmed = thinking.trim();
  if (!trimmed) {
    return null;
  }
  if (trimmed.length <= MAX_THINKING_CHARS) {
    return trimmed;
  }
  return `${trimmed.slice(0, MAX_THINKING_CHARS)}…`;
}

/// Fallback models shown when no OpenRouter key is set or the fetch fails.
export const FALLBACK_MODELS: Model[] = [
  { id: "openai/gpt-4o", name: "GPT-4o", provider: "openai" },
  {
    id: "anthropic/claude-3.5-sonnet",
    name: "Claude 3.5 Sonnet",
    provider: "anthropic",
  },
  {
    id: "meta-llama/llama-3.1-70b-instruct",
    name: "Llama 3.1 70B",
    provider: "meta-llama",
  },
  { id: "google/gemini-1.5-pro", name: "Gemini 1.5 Pro", provider: "google" },
];

/// The id of the model selected until the user (or a persisted setting)
/// chooses a real one. Exported so App.tsx initializes the lifted model state
/// with the same default and the two cannot drift.
export const FALLBACK_MODEL_ID = FALLBACK_MODELS[0].id;
