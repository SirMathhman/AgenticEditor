/// A stubbed AI-agent chat panel. It renders a simple conversation (user and
/// agent messages), a text input, and a send button. There is no real agent
/// backend yet — sending a message appends it and replies with a canned
/// placeholder so the UI flow can be exercised end to end.

import { createSignal, onCleanup, Show } from "solid-js";

interface ChatMessage {
  role: "user" | "agent";
  text: string;
}

/// Stubbed models for the picker. A real backend will replace this list with
/// the models actually available to the agent.
const MODELS = [
  "GPT-4o",
  "Claude 3.5 Sonnet",
  "Llama 3.1 70B",
  "Gemini 1.5 Pro",
];

const WELCOME: ChatMessage = {
  role: "agent",
  text: "Hi! I'm a stub agent. Send me a message to see the conversation flow.",
};

/// Produces the agent's reply to a user message. This is the single seam that
/// a real backend will replace: swap this canned string for a call into the
/// agent orchestration (Tauri command / streaming events) without touching the
/// message state or the UI below.
function replyTo(text: string): string {
  return `You said: “${text}”. (stub reply — no agent wired up yet)`;
}

export function ChatPanel() {
  const [messages, setMessages] = createSignal<ChatMessage[]>([WELCOME]);
  const [draft, setDraft] = createSignal("");
  const [model, setModel] = createSignal(MODELS[0]);
  const [pickerOpen, setPickerOpen] = createSignal(false);

  let pickerEl: HTMLDivElement | undefined;

  /// Closes the model picker when a click lands outside of it.
  function onDocClick(e: MouseEvent) {
    if (pickerEl && !pickerEl.contains(e.target as Node)) {
      setPickerOpen(false);
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  function send() {
    const text = draft().trim();
    if (!text) {
      return;
    }
    setMessages((prev) => [
      ...prev,
      { role: "user", text },
      { role: "agent", text: replyTo(text) },
    ]);
    setDraft("");
  }

  return (
    <div class="chat">
      <div class="row chat-header">
        <h2>Agent</h2>
        <div class="model-picker" ref={(el) => (pickerEl = el)}>
          <button
            type="button"
            class="model-btn"
            aria-haspopup="listbox"
            aria-expanded={pickerOpen()}
            onClick={() => setPickerOpen(!pickerOpen())}
          >
            <span class="model-name">{model()}</span>
            <span class="model-caret">▾</span>
          </button>
          <Show when={pickerOpen()}>
            <ul class="model-menu" role="listbox">
              {MODELS.map((m) => (
                <li
                  class="model-option"
                  classList={{ selected: m === model() }}
                  role="option"
                  aria-selected={m === model()}
                  onClick={() => {
                    setModel(m);
                    setPickerOpen(false);
                  }}
                >
                  {m}
                </li>
              ))}
            </ul>
          </Show>
        </div>
      </div>
      <ul class="chat-messages">
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
          send();
        }}
      >
        <input
          type="text"
          class="chat-field"
          placeholder="Message the agent…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
        />
        <button type="submit" class="btn-primary" disabled={!draft().trim()}>
          Send
        </button>
      </form>
    </div>
  );
}
