/// A stubbed AI-agent chat panel. It renders a simple conversation (user and
/// agent messages), a text input, and a send button. There is no real agent
/// backend yet — sending a message appends it and replies with a canned
/// placeholder so the UI flow can be exercised end to end.

import { createSignal } from "solid-js";

interface ChatMessage {
  role: "user" | "agent";
  text: string;
}

const WELCOME: ChatMessage = {
  role: "agent",
  text: "Hi! I'm a stub agent. Send me a message to see the conversation flow.",
};

export function ChatPanel() {
  const [messages, setMessages] = createSignal<ChatMessage[]>([WELCOME]);
  const [draft, setDraft] = createSignal("");

  function send() {
    const text = draft().trim();
    if (!text) {
      return;
    }
    setMessages((prev) => [
      ...prev,
      { role: "user", text },
      {
        role: "agent",
        text: `You said: “${text}”. (stub reply — no agent wired up yet)`,
      },
    ]);
    setDraft("");
  }

  return (
    <div class="chat">
      <div class="row">
        <h2>Agent</h2>
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
