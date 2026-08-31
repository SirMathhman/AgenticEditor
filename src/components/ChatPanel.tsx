/// An AI-agent chat panel. It renders a simple conversation (user and agent
/// messages), a text input, a send button, and a model picker. The model picker
/// shows the real models available on the user's OpenRouter account (fetched
/// via the backend); when no API key is set or the fetch fails, it falls back
/// to a small stubbed list so the UI stays usable. The API key itself is
/// managed on the Settings page — this panel only shows whether one is set.
/// There is no real agent backend yet — sending a message replies with a
/// canned placeholder.

import {
  createEffect,
  createSignal,
  onCleanup,
  Show,
  type Accessor,
} from "solid-js";
import { listModels, type Model } from "../lib/ipc";

interface ChatMessage {
  role: "user" | "agent";
  text: string;
}

/// Fallback models shown when no OpenRouter key is set or the fetch fails.
const FALLBACK_MODELS: Model[] = [
  { id: "openai/gpt-4o", name: "GPT-4o" },
  { id: "anthropic/claude-3.5-sonnet", name: "Claude 3.5 Sonnet" },
  { id: "meta-llama/llama-3.1-70b-instruct", name: "Llama 3.1 70B" },
  { id: "google/gemini-1.5-pro", name: "Gemini 1.5 Pro" },
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

export function ChatPanel(props: { keyMasked: Accessor<string> }) {
  const [messages, setMessages] = createSignal<ChatMessage[]>([WELCOME]);
  const [draft, setDraft] = createSignal("");

  // Model picker state.
  const [models, setModels] = createSignal<Model[]>(FALLBACK_MODELS);
  const [modelId, setModelId] = createSignal(FALLBACK_MODELS[0].id);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(0);
  const [modelsLoading, setModelsLoading] = createSignal(false);
  const [modelsError, setModelsError] = createSignal("");

  let pickerEl: HTMLDivElement | undefined;
  let menuEl: HTMLUListElement | undefined;

  /// Closes the model picker when a click lands outside of it.
  function onDocClick(e: MouseEvent) {
    if (pickerEl && !pickerEl.contains(e.target as Node)) {
      setPickerOpen(false);
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
      } else {
        setModels(FALLBACK_MODELS);
        setModelId(FALLBACK_MODELS[0].id);
      }
    } catch (err) {
      // No key, or the request failed — keep the fallback list and surface a
      // short note so the user knows real models weren't loaded.
      setModels(FALLBACK_MODELS);
      setModelId(FALLBACK_MODELS[0].id);
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

  function openPicker() {
    setActiveIndex(
      Math.max(
        0,
        models().findIndex((m) => m.id === modelId()),
      ),
    );
    setPickerOpen(true);
    // Move focus into the listbox so arrow keys work immediately.
    requestAnimationFrame(() => {
      menuEl?.querySelector<HTMLElement>(".model-option")?.focus();
    });
  }

  function closePicker() {
    setPickerOpen(false);
    pickerEl?.querySelector<HTMLElement>(".model-btn")?.focus();
  }

  /// Moves the active option and scrolls it into view.
  function moveActive(next: number) {
    setActiveIndex(next);
    requestAnimationFrame(() => {
      menuEl
        ?.querySelectorAll<HTMLElement>(".model-option")
        [next]?.scrollIntoView({ block: "nearest" });
    });
  }

  /// Keyboard navigation for the listbox: arrows move the active option,
  /// Enter selects it, Escape closes and returns focus to the button.
  function onMenuKeydown(e: KeyboardEvent) {
    const count = models().length;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        moveActive((activeIndex() + 1) % count);
        break;
      case "ArrowUp":
        e.preventDefault();
        moveActive((activeIndex() - 1 + count) % count);
        break;
      case "Enter":
        e.preventDefault();
        setModelId(models()[activeIndex()].id);
        closePicker();
        break;
      case "Escape":
        e.preventDefault();
        closePicker();
        break;
    }
  }

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
            onClick={() => (pickerOpen() ? setPickerOpen(false) : openPicker())}
          >
            <span class="model-name">{selectedModel().name}</span>
            <span class="model-caret">▾</span>
          </button>
          <Show when={pickerOpen()}>
            <ul
              class="model-menu"
              role="listbox"
              ref={(el) => (menuEl = el)}
              onKeyDown={onMenuKeydown}
            >
              {models().map((m, i) => (
                <li
                  class="model-option"
                  classList={{
                    selected: m.id === modelId(),
                    active: i === activeIndex(),
                  }}
                  role="option"
                  aria-selected={m.id === modelId()}
                  tabIndex={i === activeIndex() ? 0 : -1}
                  onClick={() => {
                    setModelId(m.id);
                    closePicker();
                  }}
                >
                  {m.name}
                </li>
              ))}
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
