/// An AI-agent chat panel. It renders a conversation (user and agent messages),
/// a text input, and a model picker. The model picker shows the real models
/// available on the user's OpenRouter account (fetched via the backend); when
/// no API key is set or the fetch fails, it falls back to a small stubbed list
/// so the UI stays usable. The API key itself is managed on the Settings page —
/// this panel only shows whether one is set. Sending a message calls the
/// chat-completion command with the selected model and the conversation so far,
/// and appends the assistant's reply (or an error note if the request fails).
///
/// The panel is a thin orchestrator: presentation concerns live in focused
/// sub-components under `./chat/` (ModelPicker, MessageList, SessionPicker,
/// AgentPicker, ToolsPopover). This file owns the session state and the
/// send/stream/compact logic that ties them together.

import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  Show,
  untrack,
  type Accessor,
  type Setter,
} from "solid-js";
import {
  chat,
  compactHistory,
  getProjectSessions,
  listenChatChunk,
  listenChatSubagent,
  listenChatTool,
  saveProjectSessions,
  type ChatMessage as IpcChatMessage,
  type ChatSession,
  type CustomAgent,
  type Model,
  type Provider,
  type TokenUsage,
} from "../lib/ipc";
import { AgentPicker } from "./chat/AgentPicker";
import { MessageList } from "./chat/MessageList";
import { ModelPicker } from "./chat/ModelPicker";
import { ProviderToggle } from "./chat/ProviderToggle";
import { SessionPicker } from "./chat/SessionPicker";
import { ToolsPopover } from "./chat/ToolsPopover";
import {
  capThinking,
  FALLBACK_MODEL_ID,
  type ChatMessage,
  type SubagentData,
  type ToolCall,
} from "./chat/types";

/// Re-exported so App.tsx can initialize the lifted model state with the same
/// default the picker falls back to, and the two cannot drift.
export { FALLBACK_MODEL_ID };

export function ChatPanel(props: {
  keyMasked: Accessor<string>;
  rootPath: Accessor<string>;
  agents: Accessor<CustomAgent[]>;
  setAgents: (agents: CustomAgent[]) => void;
  activeAgentId: Accessor<string | null>;
  setActiveAgentId: (id: string | null) => void;
  // Model state is lifted: App owns the persisted settings shape, ChatPanel
  // only edits it through these props.
  modelId: Accessor<string>;
  setModelId: Setter<string>;
  modelChosen: Accessor<boolean>;
  setModelChosen: (v: boolean) => void;
  // The active provider and (for llama.cpp) its base URL. Both are owned by
  // App; the panel reloads its model list when either changes. The panel also
  // writes the provider back when restoring a chat's last-used model.
  provider: Accessor<Provider>;
  setProvider: Setter<Provider>;
  baseUrl: Accessor<string>;
}) {
  // Chat sessions. `sessions` holds the conversations for the current project
  // (root folder); the active one is selected by `activeSessionId` (null when
  // none is open yet). Sessions are stored per project, outside the project
  // directory, and only exist while a folder is open.
  const [sessions, setSessions] = createSignal<ChatSession[]>([]);
  const [activeSessionId, setActiveSessionId] = createSignal<string | null>(
    null,
  );
  // Set once the current project's sessions have been loaded, so the save
  // effect doesn't clobber them with an empty list before the load resolves.
  const [sessionsLoaded, setSessionsLoaded] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  /// The subagent currently running (null when none). Its streamed output is
  /// shown in a live panel at the bottom of the message list until the
  /// `spawn_subagent` tool call completes, at which point it is folded into
  /// that tool call and this signal is cleared.
  const [liveSubagent, setLiveSubagent] = createSignal<SubagentData | null>(
    null,
  );

  // Token usage from the last completed reply (API-reported, accurate).
  const [tokenUsage, setTokenUsage] = createSignal<TokenUsage | null>(null);

  /// The model object currently selected in the picker. Owned by ModelPicker,
  /// which reports it here via `onSelectedModel`; the panel uses it for
  /// context-window math and to send requests.
  const [selectedModel, setSelectedModel] = createSignal<Model | null>(null);

  /// True while a project (root folder) is open. Chat sessions only exist for
  /// an open project, so the chat is disabled otherwise.
  const hasProject = createMemo(() => props.rootPath() !== "");

  /// The session currently being viewed, if any.
  const activeSession = createMemo<ChatSession | undefined>(() =>
    sessions().find((s) => s.id === activeSessionId()),
  );

  /// The messages of the active session (empty when none is active).
  const messages = createMemo<ChatMessage[]>(
    () => activeSession()?.messages ?? [],
  );

  /// The context window size of the selected model, in tokens.
  const contextWindow = createMemo(
    () => selectedModel()?.context_length ?? 128_000,
  );

  /// The percentage of the context window used by the last reply (0-100).
  const contextPercent = createMemo(() => {
    const usage = tokenUsage();
    if (!usage) return null;
    return Math.min(
      100,
      Math.round((usage.prompt_tokens / contextWindow()) * 100),
    );
  });

  /// True when the context is at or above the compaction threshold (80%).
  const shouldCompact = createMemo(() => {
    const pct = contextPercent();
    return pct !== null && pct >= 80;
  });

  /// The prompt of the currently active custom agent, or `null` for default.
  const activeAgentPrompt = createMemo(() => {
    const id = props.activeAgentId();
    if (!id) return null;
    return props.agents().find((a) => a.id === id)?.prompt ?? null;
  });

  // Load the current project's sessions whenever the root folder changes.
  // When no folder is open, the session list is cleared.
  createEffect(() => {
    const root = props.rootPath();
    if (!root) {
      setSessions([]);
      setActiveSessionId(null);
      setSessionsLoaded(true);
      return;
    }
    setSessionsLoaded(false);
    void getProjectSessions(root).then((loaded) => {
      if (props.rootPath() !== root) {
        return;
      }
      setSessions(loaded);
      // Reopen the most recent session, if any.
      setActiveSessionId(loaded[0]?.id ?? null);
      setSessionsLoaded(true);
    });
  });

  // When the active chat changes, restore its last-used model and provider
  // into the picker so new messages continue with that chat's model. A chat
  // without a recorded model (e.g. created before this feature) leaves the
  // current selection untouched. `untrack` keeps the effect from depending on
  // the session list it reads, so streaming updates to the active session
  // don't re-trigger the restore.
  createEffect(() => {
    const id = activeSessionId();
    if (!id || !sessionsLoaded()) return;
    const session = untrack(() => sessions().find((s) => s.id === id));
    if (!session) return;
    if (session.provider) props.setProvider(session.provider);
    if (session.model_id) props.setModelId(session.model_id);
  });

  // Persist the current project's sessions whenever they change (after the
  // initial load for that project). The frontend owns the complete session
  // list and sends it whole, so this is a plain write with no lost-update
  // race.
  // Debounced so that streaming (which updates the active session on every
  // chunk) doesn't trigger a disk write per token.
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const root = props.rootPath();
    const list = sessions();
    if (root && sessionsLoaded()) {
      if (saveTimer) {
        clearTimeout(saveTimer);
      }
      saveTimer = setTimeout(() => {
        void saveProjectSessions(root, list);
      }, 500);
    }
  });
  onCleanup(() => {
    if (saveTimer) {
      clearTimeout(saveTimer);
    }
  });

  /// Builds a new session object with a unique id. The title defaults to
  /// "New chat" and the message list to empty.
  function createSession(
    title = "New chat",
    msgs: ChatMessage[] = [],
  ): ChatSession {
    return { id: crypto.randomUUID(), title, messages: msgs };
  }

  /// Creates a new empty session and makes it active.
  function newSession() {
    const session = createSession();
    setSessions((prev) => [session, ...prev]);
    setActiveSessionId(session.id);
  }

  /// Deletes a session. If it was active, the most recent remaining session
  /// becomes active (or none, if the list is now empty).
  function deleteSession(id: string) {
    setSessions((prev) => {
      const next = prev.filter((s) => s.id !== id);
      if (id === activeSessionId()) {
        setActiveSessionId(next[0]?.id ?? null);
      }
      return next;
    });
  }

  /// Appends a message to the active session, creating one if none is active.
  /// The session title is derived from the first user message.
  function appendToActiveSession(msg: ChatMessage) {
    const activeId = activeSessionId();
    if (activeId === null) {
      // No session yet (e.g. a fresh project): create one and make it active.
      const title = msg.role === "user" ? msg.text.slice(0, 40) : "New chat";
      const session = createSession(title, [msg]);
      setSessions((prev) => [session, ...prev]);
      setActiveSessionId(session.id);
      return;
    }
    setSessions((prev) =>
      prev.map((s) => {
        if (s.id !== activeId) {
          return s;
        }
        const isFirstUser =
          msg.role === "user" && !s.messages.some((m) => m.role === "user");
        return {
          ...s,
          title: isFirstUser ? msg.text.slice(0, 40) : s.title,
          messages: [...s.messages, msg],
        };
      }),
    );
  }

  /// Replaces the last message of the active session in place. Used to grow
  /// the streaming agent message as chunks arrive, without appending a new
  /// message per chunk.
  function updateActiveSessionLastMessage(
    update: (msg: ChatMessage) => ChatMessage,
  ) {
    const activeId = activeSessionId();
    if (activeId === null) {
      return;
    }
    setSessions((prev) =>
      prev.map((s) => {
        if (s.id !== activeId || s.messages.length === 0) {
          return s;
        }
        const last = s.messages[s.messages.length - 1];
        return {
          ...s,
          messages: [...s.messages.slice(0, -1), update(last)],
        };
      }),
    );
  }

  /// Records the currently selected model and provider on the active session
  /// so that reopening the chat restores them into the picker.
  function stampActiveSessionModel() {
    const activeId = activeSessionId();
    if (activeId === null) return;
    const modelId = selectedModel()?.id;
    const provider = props.provider();
    setSessions((prev) =>
      prev.map((s) =>
        s.id === activeId ? { ...s, model_id: modelId ?? null, provider } : s,
      ),
    );
  }

  /// Compacts the conversation by summarizing it into a single system
  /// message. Called when the context window is nearly full.
  async function compactConversation() {
    const msgs = messages();
    if (msgs.length === 0) return;
    const history: IpcChatMessage[] = msgs.map((m) => ({
      role: (m.role === "agent"
        ? "assistant"
        : m.role === "system"
          ? "system"
          : "user") as IpcChatMessage["role"],
      content: m.text,
    }));
    try {
      const summary = await compactHistory(selectedModel()?.id ?? "", history);
      // Replace the entire conversation with the summary.
      const activeId = activeSessionId();
      if (activeId === null) return;
      setSessions((prev) =>
        prev.map((s) => {
          if (s.id !== activeId) return s;
          return {
            ...s,
            messages: [{ role: "system" as const, text: summary }],
          };
        }),
      );
    } catch {
      // If summarization fails, proceed without compaction. The next turn
      // may fail with a context error, but that's better than losing the
      // user's message.
    }
  }

  /// Sends the draft to the selected model and appends the assistant's reply.
  /// The whole conversation is sent each turn so the model has context.
  /// If the context window is nearly full, the conversation is compacted
  /// (summarized) before sending.
  async function send() {
    const text = draft().trim();
    if (!text || busy() || !hasProject()) {
      return;
    }
    // Compact the conversation if the context is nearly full.
    if (shouldCompact()) {
      setBusy(true);
      await compactConversation();
    }
    // Build the request from the conversation so far plus the new user turn.
    const history: IpcChatMessage[] = [
      ...messages().map((m) => ({
        role: (m.role === "agent"
          ? "assistant"
          : m.role === "system"
            ? "system"
            : "user") as IpcChatMessage["role"],
        content: m.text,
      })),
      { role: "user", content: text },
    ];
    appendToActiveSession({ role: "user", text });
    // Remember which model/provider this chat is using so it's restored on
    // the next load.
    stampActiveSessionModel();
    setDraft("");
    setBusy(true);
    // Accumulate the streamed reply. The agent message is created on the first
    // chunk and grown in place afterwards, so an error before any token
    // arrives doesn't leave an empty bubble behind.
    let content = "";
    let reasoning = "";
    let toolCalls: ToolCall[] = [];
    let seeded = false;
    // The subagent currently running, accumulated from its `chat:subagent`
    // events. It is shown live via the `liveSubagent` signal and, once the
    // `spawn_subagent` tool call completes, folded into that tool call.
    let pendingSubagent: SubagentData | null = null;
    let unlistenChunk: (() => void) | undefined;
    let unlistenTool: (() => void) | undefined;
    let unlistenSubagent: (() => void) | undefined;
    try {
      unlistenChunk = await listenChatChunk((chunk) => {
        content += chunk.content;
        reasoning += chunk.reasoning;
        const thinking = capThinking(reasoning);
        if (!seeded) {
          seeded = true;
          // Seed with the tool calls accumulated so far: a tool round can
          // finish before the first text chunk of the final round arrives,
          // so those events must not be lost.
          appendToActiveSession({
            role: "agent",
            text: content,
            thinking,
            toolCalls: [...toolCalls],
          });
        } else {
          updateActiveSessionLastMessage((msg) => ({
            ...msg,
            text: content,
            thinking,
          }));
        }
      });
      unlistenSubagent = await listenChatSubagent((event) => {
        // Mirror the subagent's progress into `pendingSubagent` (for folding
        // into the tool call) and the `liveSubagent` signal (for live render).
        if (event.type === "start") {
          pendingSubagent = {
            role: event.role,
            content: "",
            reasoning: "",
            tools: [],
            result: null,
          };
        } else if (!pendingSubagent) {
          // A chunk/tool/end before a start is malformed; ignore it.
          return;
        } else if (event.type === "chunk") {
          pendingSubagent = {
            ...pendingSubagent,
            content: pendingSubagent.content + event.content,
            reasoning: pendingSubagent.reasoning + event.reasoning,
          };
        } else if (event.type === "tool") {
          pendingSubagent = {
            ...pendingSubagent,
            tools: [
              ...pendingSubagent.tools,
              { name: event.name, result: event.result },
            ],
          };
        } else {
          // end
          pendingSubagent = {
            ...pendingSubagent,
            result: event.result,
          };
        }
        setLiveSubagent(pendingSubagent);
      });
      unlistenTool = await listenChatTool((tool) => {
        // When the `spawn_subagent` call completes, attach the accumulated
        // subagent output to it and stop showing the live panel.
        let entry: ToolCall = tool;
        if (tool.name === "spawn_subagent" && pendingSubagent) {
          entry = { ...tool, subagent: pendingSubagent };
          pendingSubagent = null;
          setLiveSubagent(null);
        }
        toolCalls = [...toolCalls, entry];
        updateActiveSessionLastMessage((msg) => ({
          ...msg,
          toolCalls: [...toolCalls],
        }));
      });
      const reply = await chat(
        selectedModel()?.id ?? "",
        history,
        activeAgentPrompt(),
      );
      if (reply.usage) {
        setTokenUsage(reply.usage);
      }
    } catch (err) {
      appendToActiveSession({ role: "agent", text: `⚠️ ${String(err)}` });
    } finally {
      unlistenChunk?.();
      unlistenTool?.();
      unlistenSubagent?.();
      // Clear any subagent that didn't get folded into a tool call (e.g. the
      // request errored mid-spawn) so the live panel doesn't linger.
      setLiveSubagent(null);
      setBusy(false);
    }
  }

  return (
    <div class="chat">
      <div class="row chat-header">
        <h2>Agent</h2>
        <SessionPicker
          hasProject={hasProject}
          sessions={sessions}
          activeSessionId={activeSessionId}
          setActiveSessionId={setActiveSessionId}
          newSession={newSession}
          deleteSession={deleteSession}
        />
        <ToolsPopover />
        <AgentPicker
          agents={props.agents}
          activeAgentId={props.activeAgentId}
          setActiveAgentId={props.setActiveAgentId}
        />
        <ProviderToggle provider={props.provider} setProvider={props.setProvider} />
        <ModelPicker
          modelId={props.modelId}
          setModelId={props.setModelId}
          setModelChosen={props.setModelChosen}
          provider={props.provider}
          keyMasked={props.keyMasked}
          baseUrl={props.baseUrl}
          onSelectedModel={setSelectedModel}
        />
        <Show when={tokenUsage()}>
          <div
            class="context-usage"
            classList={{ "context-usage-warn": shouldCompact() }}
            title={`Prompt: ${tokenUsage()!.prompt_tokens.toLocaleString()} tokens\nCompletion: ${tokenUsage()!.completion_tokens.toLocaleString()} tokens\nWindow: ${contextWindow().toLocaleString()} tokens`}
          >
            <span class="context-usage-label">ctx</span>
            <span class="context-usage-value">{contextPercent()}%</span>
          </div>
        </Show>
      </div>

      <div class="chat-key">
        <Show
          when={props.keyMasked()}
          fallback={
            <span class="key-note">No OpenRouter key — set it in Settings</span>
          }
        >
          <span class="key-status" title="OpenRouter key saved">
            🔑 {props.keyMasked()}
          </span>
        </Show>
      </div>

      <MessageList
        hasProject={hasProject}
        activeSessionId={activeSessionId}
        messages={messages}
        liveSubagent={liveSubagent}
      />

      <form
        class="chat-input"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <input
          type="text"
          class="chat-field"
          placeholder={
            hasProject() ? "Message the agent…" : "Open a folder to chat…"
          }
          value={draft()}
          disabled={!hasProject()}
          onInput={(e) => setDraft(e.currentTarget.value)}
        />
        <button
          type="submit"
          class="btn-primary"
          disabled={!hasProject() || !draft().trim() || busy()}
        >
          {busy() ? "…" : "Send"}
        </button>
      </form>
    </div>
  );
}
