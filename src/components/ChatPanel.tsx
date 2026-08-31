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
  onCleanup,
  onMount,
  Show,
  type Accessor,
} from "solid-js";
import {
  chat,
  getSettings,
  listModels,
  saveSettings,
  type ChatMessage as IpcChatMessage,
  type ChatSession,
  type Model,
} from "../lib/ipc";

interface ChatMessage {
  role: "user" | "agent";
  text: string;
}

/// A provider group in the model picker: the provider label plus its models
/// in their original order.
interface ModelGroup {
  provider: string;
  models: Model[];
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

export function ChatPanel(props: { keyMasked: Accessor<string> }) {
  // Chat sessions. `sessions` holds every persisted conversation; the active
  // one is selected by `activeSessionId` (null when none is open yet).
  const [sessions, setSessions] = createSignal<ChatSession[]>([]);
  const [activeSessionId, setActiveSessionId] = createSignal<string | null>(
    null,
  );
  // Set once the persisted settings have been loaded, so the save effect below
  // doesn't overwrite them with defaults before the load resolves.
  const [settingsLoaded, setSettingsLoaded] = createSignal(false);
  const [sessionPickerOpen, setSessionPickerOpen] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  /// The session currently being viewed, if any.
  const activeSession = createMemo<ChatSession | undefined>(() =>
    sessions().find((s) => s.id === activeSessionId()),
  );

  /// The messages of the active session (empty when none is active).
  const messages = createMemo<ChatMessage[]>(
    () => activeSession()?.messages ?? [],
  );

  // Model picker state.
  const [models, setModels] = createSignal<Model[]>(FALLBACK_MODELS);
  const [modelId, setModelId] = createSignal(FALLBACK_MODELS[0].id);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(0);
  // The text in the combobox input. When the picker is closed it mirrors the
  // selected model's name; while open it is the user's (case-insensitive)
  // filter query.
  const [query, setQuery] = createSignal("");
  const [modelsLoading, setModelsLoading] = createSignal(false);
  const [modelsError, setModelsError] = createSignal("");
  // True once the user has explicitly chosen a model (or a real model list
  // has loaded). Until then the model id is a fallback default and should not
  // be persisted.
  const [modelChosen, setModelChosen] = createSignal(false);

  // Restore the last selected model and the persisted sessions on startup.
  onMount(() => {
    void getSettings().then((s) => {
      if (s.model_id) {
        setModelId(s.model_id);
        setModelChosen(true);
      }
      setSessions(s.sessions);
      // Reopen the most recent session, if any.
      if (s.sessions.length > 0) {
        setActiveSessionId(s.sessions[0].id);
      }
      setSettingsLoaded(true);
    });
  });

  // Persist the full settings (model id + sessions) whenever either changes,
  // in a single write. The frontend owns the complete settings state, so this
  // avoids the lost-update race of two independent read-modify-write saves.
  // `model_id` is only persisted once the user has actually chosen a model
  // (or a real model list has loaded); until then it stays null so the
  // fallback default is never written to disk.
  createEffect(() => {
    if (settingsLoaded()) {
      void saveSettings({
        model_id: modelChosen() ? modelId() : null,
        sessions: sessions(),
      });
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

  /// Closes the model picker when a click lands outside of it.
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
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  /// The currently selected model object (falls back to the first if the id
  /// is not in the list, e.g. after a model list refresh).
  function selectedModel(): Model {
    return models().find((m) => m.id === modelId()) ?? models()[0];
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
        setModelId((prev) =>
          real.some((m) => m.id === prev) ? prev : real[0].id,
        );
        // A real model is now selected, so it is safe to persist.
        setModelChosen(true);
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
        flatModels().findIndex((m) => m.id === modelId()),
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
    setModelId(m.id);
    setModelChosen(true);
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

  /// Creates a new empty session and makes it active.
  function newSession() {
    const session: ChatSession = {
      id: crypto.randomUUID(),
      title: "New chat",
      messages: [],
    };
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
    setSessions((prev) =>
      prev.map((s) => {
        if (s.id !== activeSessionId()) {
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

  /// Sends the draft to the selected model and appends the assistant's reply.
  /// The whole conversation is sent each turn so the model has context.
  async function send() {
    const text = draft().trim();
    if (!text || busy()) {
      return;
    }
    // Build the request from the conversation so far plus the new user turn.
    const history: IpcChatMessage[] = [
      ...messages().map((m) => ({
        role: (m.role === "agent"
          ? "assistant"
          : "user") as IpcChatMessage["role"],
        content: m.text,
      })),
      { role: "user", content: text },
    ];
    appendToActiveSession({ role: "user", text });
    setDraft("");
    setBusy(true);
    try {
      const reply = await chat(selectedModel().id, history);
      appendToActiveSession({ role: "agent", text: reply });
    } catch (err) {
      appendToActiveSession({ role: "agent", text: `⚠️ ${String(err)}` });
    } finally {
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
                            selected: m.id === modelId(),
                            active: i === activeIndex(),
                          }}
                          role="option"
                          id={`model-option-${i}`}
                          aria-selected={m.id === modelId()}
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

      <ul class="chat-messages">
        <Show when={messages().length === 0}>
          <li class="chat-empty">
            Start a conversation — your chats are saved and can be reopened.
          </li>
        </Show>
        {messages().map((m) => (
          <li class="chat-msg" classList={{ [m.role]: true }}>
            <span class="chat-role">{m.role === "user" ? "You" : "Agent"}</span>
            <span class="chat-text">{m.text}</span>
          </li>
        ))}
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
          placeholder="Message the agent…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
        />
        <button
          type="submit"
          class="btn-primary"
          disabled={!draft().trim() || busy()}
        >
          {busy() ? "…" : "Send"}
        </button>
      </form>
    </div>
  );
}
