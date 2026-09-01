/// An AI-agent chat panel. It renders a simple conversation (user and agent
/// messages), a text input, a send button, and a model picker. The model picker
/// shows the real models available on the user's OpenRouter account (fetched
/// via the backend); when no API key is set or the fetch fails, it falls back
/// to a small stubbed list so the UI stays usable. The API key itself is
/// managed on the Settings page — this panel only shows whether one is set.
/// Sending a message calls the OpenRouter chat-completion command with the
/// selected model and the conversation so far, and appends the assistant's
/// reply (or an error note if the request fails).

import {
  createEffect,
  createMemo,
  createSignal,
  Index,
  onCleanup,
  onMount,
  Setter,
  Show,
  type Accessor,
} from "solid-js";
import {
  chat,
  compactHistory,
  getProjectSessions,
  listenChatChunk,
  listenChatSubagent,
  listenChatTool,
  listModels,
  listTools,
  saveProjectSessions,
  type ChatMessage as IpcChatMessage,
  type ChatSession,
  type CustomAgent,
  type Model,
  type TokenUsage,
  type ToolInfo,
} from "../lib/ipc";

/// The streamed output of a subagent spawned via the `spawn_subagent` tool.
/// Accumulated live while the subagent runs and then attached to the
/// `spawn_subagent` tool call so it renders nested inside that call.
interface SubagentData {
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
interface ToolCall {
  name: string;
  result: string;
  /// Present only on `spawn_subagent` calls: the subagent's own streamed
  /// output, rendered nested inside the tool call.
  subagent?: SubagentData;
}

interface ChatMessage {
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
interface ModelGroup {
  provider: string;
  models: Model[];
}

/// The maximum length of a model's chain-of-thought kept for display and
/// persistence. Reasoning models can emit very long thinking; capping it keeps
/// session files from growing unbounded. Longer output is truncated with an
/// ellipsis marker.
const MAX_THINKING_CHARS = 4000;

/// Truncates a chain-of-thought to `MAX_THINKING_CHARS`, appending a marker
/// when it was cut. Returns `null` for empty/whitespace-only input.
function capThinking(thinking: string | null | undefined): string | null {
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
const FALLBACK_MODELS: Model[] = [
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
}) {
  // Chat sessions. `sessions` holds the conversations for the current project
  // (root folder); the active one is selected by `activeSessionId` (null when
  // none is open yet). Sessions are stored per project, outside the project
  // directory, and only exist while a folder is open.
  const [sessions, setSessions] = createSignal<ChatSession[]>([]);
  const [activeSessionId, setActiveSessionId] = createSignal<string | null>(
    null,
  );
  let messagesEl: HTMLUListElement | undefined;
  // Set once the current project's sessions have been loaded, so the save
  // effect doesn't clobber them with an empty list before the load resolves.
  const [sessionsLoaded, setSessionsLoaded] = createSignal(false);
  const [sessionPickerOpen, setSessionPickerOpen] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  /// The subagent currently running (null when none). Its streamed output is
  /// shown in a live panel at the bottom of the message list until the
  /// `spawn_subagent` tool call completes, at which point it is folded into
  /// that tool call and this signal is cleared.
  const [liveSubagent, setLiveSubagent] = createSignal<SubagentData | null>(
    null,
  );

  /// True while a project (root folder) is open. Chat sessions only exist for
  /// an open project, so the chat is disabled otherwise.
  const hasProject = createMemo(() => props.rootPath() !== "");

  // Scroll to the bottom when the active session changes (opening a stored
  // chat or switching between chats). Only tracks `activeSessionId`, so it
  // does not re-run on every streaming chunk. `requestAnimationFrame` ensures
  // the new messages are painted before we measure `scrollHeight`.
  createEffect(() => {
    if (activeSessionId() && messagesEl) {
      requestAnimationFrame(() => {
        if (messagesEl) {
          messagesEl.scrollTop = messagesEl.scrollHeight;
        }
      });
    }
  });

  /// The session currently being viewed, if any.
  const activeSession = createMemo<ChatSession | undefined>(() =>
    sessions().find((s) => s.id === activeSessionId()),
  );

  /// The messages of the active session (empty when none is active).
  const messages = createMemo<ChatMessage[]>(
    () => activeSession()?.messages ?? [],
  );

  // Model picker state. `modelId`/`modelChosen` are lifted to App (the
  // settings owner) and arrive via props.
  const [models, setModels] = createSignal<Model[]>(FALLBACK_MODELS);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(0);
  // The text in the combobox input. When the picker is closed it mirrors the
  // selected model's name; while open it is the user's (case-insensitive)
  // filter query.
  const [query, setQuery] = createSignal("");
  const [modelsLoading, setModelsLoading] = createSignal(false);
  const [modelsError, setModelsError] = createSignal("");
  // The tools the agent can call, shown in the "Tools" popover. Static for a
  // given build (the registry is fixed), so it is loaded once on mount.
  const [tools, setTools] = createSignal<ToolInfo[]>([]);
  const [toolsOpen, setToolsOpen] = createSignal(false);
  let toolsEl: HTMLDivElement | undefined;

  // Custom agents: user-defined system prompts. Owned by App.tsx so the
  // settings page and chat panel share one source of truth.
  const [agentPickerOpen, setAgentPickerOpen] = createSignal(false);
  let agentPickerEl: HTMLDivElement | undefined;

  // Token usage from the last completed reply (API-reported, accurate).
  const [tokenUsage, setTokenUsage] = createSignal<TokenUsage | null>(null);

  /// The context window size of the selected model, in tokens.
  const contextWindow = createMemo(
    () => selectedModel().context_length ?? 128_000,
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

  // Restore the agent's tool list on mount. The selected model is restored
  // (and settings persisted) by App.tsx, the single owner of the settings
  // shape. Custom agents are loaded by App.tsx too.
  onMount(() => {
    void listTools().then(setTools);
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

  // While the picker is closed, keep the input text in sync with the selected
  // model's name (covers the initial value and any external model change).
  // `closing` guards against a race: when the user opens the picker and types
  // quickly, this effect must not clobber the filter text.
  let closing = true;
  createEffect(() => {
    if (!pickerOpen() && closing) {
      setQuery(selectedModel().name);
    }
  });

  /// The models matching the current (case-insensitive) query, in original
  /// order. An empty query matches everything.
  const filteredModels = createMemo(() => {
    const q = query().trim().toLowerCase();
    if (!q) {
      return models();
    }
    return models().filter(
      (m) =>
        m.name.toLowerCase().includes(q) ||
        m.id.toLowerCase().includes(q) ||
        m.provider.toLowerCase().includes(q),
    );
  });

  /// The filtered models grouped by provider, in first-seen order.
  const groupedModels = createMemo(() => {
    const groups: ModelGroup[] = [];
    const byProvider = new Map<string, ModelGroup>();
    for (const m of filteredModels()) {
      const provider = m.provider;
      let group = byProvider.get(provider);
      if (!group) {
        group = { provider, models: [] };
        byProvider.set(provider, group);
        groups.push(group);
      }
      group.models.push(m);
    }
    return groups;
  });

  /// The filtered models flattened in the same order the options render
  /// (grouped by provider). Keyboard navigation indexes into this so the
  /// active highlight and the Enter-selection always agree.
  const flatModels = createMemo(() => groupedModels().flatMap((g) => g.models));

  let pickerEl: HTMLDivElement | undefined;
  let menuEl: HTMLUListElement | undefined;
  let inputEl: HTMLInputElement | undefined;
  let sessionPickerEl: HTMLDivElement | undefined;

  /// Closes the model picker, tools popover, and agent picker when a click
  /// lands outside of them.
  function onDocClick(e: MouseEvent) {
    if (pickerEl && !pickerEl.contains(e.target as Node)) {
      closePicker();
    }
    if (
      sessionPickerEl &&
      !sessionPickerEl.contains(e.target as Node) &&
      sessionPickerOpen()
    ) {
      setSessionPickerOpen(false);
    }
    if (toolsEl && !toolsEl.contains(e.target as Node) && toolsOpen()) {
      setToolsOpen(false);
    }
    if (
      agentPickerEl &&
      !agentPickerEl.contains(e.target as Node) &&
      agentPickerOpen()
    ) {
      setAgentPickerOpen(false);
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  /// The currently selected model object (falls back to the first if the id
  /// is not in the list, e.g. after a model list refresh).
  function selectedModel(): Model {
    return models().find((m) => m.id === props.modelId()) ?? models()[0];
  }

  /// Fetches the real models for the stored key. Falls back to the stubbed
  /// list when there is no key or the request fails.
  async function loadModels() {
    setModelsLoading(true);
    setModelsError("");
    try {
      const real = await listModels();
      if (real.length > 0) {
        setModels(real);
        props.setModelId((prev) =>
          real.some((m) => m.id === prev) ? prev : real[0].id,
        );
        // A real model is now selected, so it is safe to persist.
        props.setModelChosen(true);
      } else {
        // No real models — keep the fallback list. Leave the selected id
        // untouched so a persisted (real) model id survives until the key is
        // restored; selectedModel() falls back to the first model meanwhile.
        setModels(FALLBACK_MODELS);
      }
    } catch (err) {
      // No key, or the request failed — keep the fallback list and surface a
      // short note so the user knows real models weren't loaded.
      setModels(FALLBACK_MODELS);
      setModelsError(String(err));
    } finally {
      setModelsLoading(false);
    }
  }

  // Load the real models whenever a key is present. Runs on mount and again
  // if the key is set or cleared (e.g. from the Settings page), so the picker
  // stays in sync without the panel being remounted.
  createEffect(() => {
    if (props.keyMasked()) {
      void loadModels();
    }
  });

  /// Opens the combobox: shows the full list, focuses the input, and selects
  /// its text so typing replaces it.
  function openPicker() {
    closing = false;
    setQuery("");
    setActiveIndex(
      Math.max(
        0,
        flatModels().findIndex((m) => m.id === props.modelId()),
      ),
    );
    setPickerOpen(true);
    requestAnimationFrame(() => {
      inputEl?.focus();
      inputEl?.select();
    });
  }

  /// Closes the combobox and restores the input to the selected model's name.
  function closePicker() {
    closing = true;
    setPickerOpen(false);
    setQuery(selectedModel().name);
  }

  /// Moves the active option (wrapping) and scrolls it into view.
  function moveActive(next: number) {
    const count = flatModels().length;
    if (count === 0) {
      return;
    }
    setActiveIndex(((next % count) + count) % count);
    requestAnimationFrame(() => {
      menuEl
        ?.querySelectorAll<HTMLElement>(".model-option")
        [activeIndex()]?.scrollIntoView({ block: "nearest" });
    });
  }

  /// Selects a model: sets it, closes the picker, and restores the input text.
  function selectModel(m: Model) {
    props.setModelId(m.id);
    props.setModelChosen(true);
    closePicker();
  }

  /// Handles typing in the combobox: updates the (case-insensitive) filter and
  /// opens the list if it is closed.
  function onInput(e: InputEvent) {
    setQuery((e.currentTarget as HTMLInputElement).value);
    if (!pickerOpen()) {
      openPicker();
    }
    setActiveIndex(0);
  }

  /// Keyboard handling for the combobox input: arrows move the active option,
  /// Enter selects it, Escape closes, and typing filters the list.
  function onInputKeydown(e: KeyboardEvent) {
    const count = flatModels().length;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        if (pickerOpen()) {
          moveActive(activeIndex() + 1);
        } else {
          openPicker();
        }
        break;
      case "ArrowUp":
        e.preventDefault();
        if (pickerOpen()) {
          moveActive(activeIndex() - 1);
        } else {
          openPicker();
        }
        break;
      case "Enter":
        if (pickerOpen() && count > 0) {
          e.preventDefault();
          selectModel(flatModels()[activeIndex()]);
        }
        break;
      case "Escape":
        if (pickerOpen()) {
          e.preventDefault();
          closePicker();
        }
        break;
    }
  }

  /// Builds a new session object with a unique id. The title defaults to
  /// "New chat" and the message list to empty.
  function createSession(
    title = "New chat",
    messages: ChatMessage[] = [],
  ): ChatSession {
    return { id: crypto.randomUUID(), title, messages };
  }

  /// Creates a new empty session and makes it active.
  function newSession() {
    const session = createSession();
    setSessions((prev) => [session, ...prev]);
    setActiveSessionId(session.id);
    setSessionPickerOpen(false);
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
    setSessionPickerOpen(false);
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
      const summary = await compactHistory(selectedModel().id, history);
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
        selectedModel().id,
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
        <div class="session-picker" ref={(el) => (sessionPickerEl = el)}>
          <button
            type="button"
            class="session-toggle"
            disabled={!hasProject()}
            onClick={() => setSessionPickerOpen((v) => !v)}
          >
            <span class="session-title">
              {activeSession()?.title ?? "New chat"}
            </span>
            <span class="session-caret">▾</span>
          </button>
          <Show when={sessionPickerOpen()}>
            <div class="session-menu">
              <button type="button" class="session-new" onClick={newSession}>
                + New chat
              </button>
              <ul>
                {sessions().map((s) => (
                  <li
                    class="session-item"
                    classList={{ active: s.id === activeSessionId() }}
                    onClick={() => {
                      setActiveSessionId(s.id);
                      setSessionPickerOpen(false);
                    }}
                  >
                    <span class="session-item-title">{s.title}</span>
                    <button
                      type="button"
                      class="session-delete"
                      title="Delete chat"
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteSession(s.id);
                      }}
                    >
                      ✕
                    </button>
                  </li>
                ))}
                <Show when={sessions().length === 0}>
                  <li class="session-empty">No chats yet</li>
                </Show>
              </ul>
            </div>
          </Show>
        </div>
        <div class="tools-picker" ref={(el) => (toolsEl = el)}>
          <button
            type="button"
            class="tools-toggle"
            onClick={() => setToolsOpen((v) => !v)}
            aria-haspopup="true"
            aria-expanded={toolsOpen()}
          >
            <span>Tools</span>
            <span class="tools-count">{tools().length}</span>
          </button>
          <Show when={toolsOpen()}>
            <div class="tools-menu">
              <p class="tools-menu-note">
                The agent can call these tools while it works.
              </p>
              <ul>
                <Index each={tools()}>
                  {(t) => (
                    <li class="tools-item">
                      <span class="tools-item-name">{t().name}</span>
                      <span class="tools-item-desc">{t().description}</span>
                    </li>
                  )}
                </Index>
              </ul>
            </div>
          </Show>
        </div>
        <div class="agent-picker" ref={(el) => (agentPickerEl = el)}>
          <button
            type="button"
            class="agent-toggle"
            onClick={() => setAgentPickerOpen((v) => !v)}
            aria-haspopup="true"
            aria-expanded={agentPickerOpen()}
          >
            <span class="agent-name">
              {props.agents().find((a) => a.id === props.activeAgentId())
                ?.name ?? "Default"}
            </span>
            <span class="session-caret">▾</span>
          </button>
          <Show when={agentPickerOpen()}>
            <div class="agent-menu">
              <button
                type="button"
                class="agent-item"
                classList={{ active: props.activeAgentId() === null }}
                onClick={() => {
                  props.setActiveAgentId(null);
                  setAgentPickerOpen(false);
                }}
              >
                Default
              </button>
              {props.agents().map((a) => (
                <button
                  type="button"
                  class="agent-item"
                  classList={{ active: a.id === props.activeAgentId() }}
                  onClick={() => {
                    props.setActiveAgentId(a.id);
                    setAgentPickerOpen(false);
                  }}
                >
                  {a.name}
                </button>
              ))}
              <Show when={props.agents().length === 0}>
                <p class="agent-menu-note">
                  No custom agents. Create one in Settings.
                </p>
              </Show>
            </div>
          </Show>
        </div>
        <div class="model-picker" ref={(el) => (pickerEl = el)}>
          <input
            type="text"
            class="model-input"
            role="combobox"
            aria-haspopup="listbox"
            aria-expanded={pickerOpen()}
            aria-autocomplete="list"
            aria-controls="model-listbox"
            aria-activedescendant={
              pickerOpen() ? `model-option-${activeIndex()}` : undefined
            }
            placeholder="Choose a model…"
            value={query()}
            ref={(el) => (inputEl = el)}
            onInput={onInput}
            onKeyDown={onInputKeydown}
            onFocus={openPicker}
            onBlur={() => {
              // Option clicks call preventDefault on mousedown, so this only
              // fires when focus truly leaves the input (outside click, Tab).
              if (pickerOpen()) {
                closePicker();
              }
            }}
          />
          <Show when={pickerOpen()}>
            <ul
              class="model-menu"
              role="listbox"
              id="model-listbox"
              ref={(el) => (menuEl = el)}
            >
              {groupedModels().map((g, gi) => {
                // Flat index of this group's first model, for keyboard nav.
                const base = groupedModels()
                  .slice(0, gi)
                  .reduce((n, prev) => n + prev.models.length, 0);
                return (
                  <>
                    <li class="model-group">{g.provider}</li>
                    {g.models.map((m, mi) => {
                      const i = base + mi;
                      return (
                        <li
                          class="model-option"
                          classList={{
                            selected: m.id === props.modelId(),
                            active: i === activeIndex(),
                          }}
                          role="option"
                          id={`model-option-${i}`}
                          aria-selected={m.id === props.modelId()}
                          onMouseDown={(e) => {
                            // Prevent the input from blurring before the click
                            // registers, so the option can be selected.
                            e.preventDefault();
                            selectModel(m);
                          }}
                        >
                          {g.provider} · {m.name}
                        </li>
                      );
                    })}
                  </>
                );
              })}
              <Show when={filteredModels().length === 0}>
                <li class="model-empty">No matching models</li>
              </Show>
            </ul>
          </Show>
        </div>
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
        <Show when={modelsLoading()}>
          <span class="key-note">Loading models…</span>
        </Show>
        <Show when={modelsError() && !modelsLoading()}>
          <span class="key-note error" title={modelsError()}>
            Using fallback models
          </span>
        </Show>
      </div>

      <ul class="chat-messages" ref={(el) => (messagesEl = el)}>
        <Show when={!hasProject()}>
          <li class="chat-empty">
            Open a folder to start chatting. Chats are saved per project.
          </li>
        </Show>
        <Show when={hasProject() && messages().length === 0}>
          <li class="chat-empty">
            Start a conversation — your chats are saved and can be reopened.
          </li>
        </Show>
        {/* Keyed on the session so switching chats builds a fresh list
            rather than reusing the previous session's rows (and their
            expanded/collapsed thinking panels) by index. */}
        <Show when={activeSessionId()} keyed>
          {/* <Index> keys by position, so a streaming message reuses its
              existing <li> and only its text nodes change. A plain .map()
              returns all-new DOM nodes, which Solid reconciles by node
              identity — it would replace every row on every chunk, throwing
              away the <details> element (and any click on its summary)
              several times a second. */}
          <Index each={messages()}>
            {(m) => (
              <li class="chat-msg" classList={{ [m().role]: true }}>
                <span class="chat-role">
                  {m().role === "user"
                    ? "You"
                    : m().role === "system"
                      ? "Compacted"
                      : "Agent"}
                </span>
                <Show when={m().role !== "system" && m().thinking}>
                  <details class="chat-thinking">
                    <summary>Thinking</summary>
                    <span class="chat-thinking-text">{m().thinking}</span>
                  </details>
                </Show>
                <Show when={m().role !== "system" && m().toolCalls?.length}>
                  <details class="chat-toolcalls">
                    <summary>Tool calls ({m().toolCalls!.length})</summary>
                    <ul class="chat-toolcall-list">
                      <Index each={m().toolCalls!}>
                        {(tc) => (
                          <li class="chat-toolcall">
                            <span class="chat-toolcall-name">{tc().name}</span>
                            <Show when={tc().subagent} keyed>
                              {(sub) => (
                                <div class="chat-subagent">
                                  <span class="chat-subagent-role">
                                    {sub.role}
                                  </span>
                                  <Show when={sub.reasoning}>
                                    <details class="chat-thinking">
                                      <summary>Thinking</summary>
                                      <span class="chat-thinking-text">
                                        {sub.reasoning}
                                      </span>
                                    </details>
                                  </Show>
                                  <Show when={sub.tools.length > 0}>
                                    <ul class="chat-toolcall-list">
                                      <Index each={sub.tools}>
                                        {(stc) => (
                                          <li class="chat-toolcall">
                                            <span class="chat-toolcall-name">
                                              {stc().name}
                                            </span>
                                            <span class="chat-toolcall-result">
                                              {stc().result}
                                            </span>
                                          </li>
                                        )}
                                      </Index>
                                    </ul>
                                  </Show>
                                  <span class="chat-subagent-content">
                                    {sub.content}
                                  </span>
                                </div>
                              )}
                            </Show>
                            <span class="chat-toolcall-result">
                              {tc().result}
                            </span>
                          </li>
                        )}
                      </Index>
                    </ul>
                  </details>
                </Show>
                <span class="chat-text">{m().text}</span>
              </li>
            )}
          </Index>
        </Show>
        {/* Live view of a subagent while it runs. Once its `spawn_subagent`
            tool call completes, this is cleared and the output is folded into
            that tool call above. */}
        <Show when={liveSubagent()} keyed>
          {(sub) => (
            <li class="chat-msg agent">
              <span class="chat-role">Subagent · {sub.role}</span>
              <div class="chat-subagent">
                <Show when={sub.reasoning}>
                  <details class="chat-thinking">
                    <summary>Thinking</summary>
                    <span class="chat-thinking-text">{sub.reasoning}</span>
                  </details>
                </Show>
                <Show when={sub.tools.length > 0}>
                  <ul class="chat-toolcall-list">
                    <Index each={sub.tools}>
                      {(stc) => (
                        <li class="chat-toolcall">
                          <span class="chat-toolcall-name">{stc().name}</span>
                          <span class="chat-toolcall-result">
                            {stc().result}
                          </span>
                        </li>
                      )}
                    </Index>
                  </ul>
                </Show>
                <span class="chat-subagent-content">{sub.content}</span>
              </div>
            </li>
          )}
        </Show>
      </ul>
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
