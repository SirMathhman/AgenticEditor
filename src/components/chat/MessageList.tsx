/// The conversation view: renders the active session's messages (with their
/// thinking panels and tool calls), the empty-state hints, and a live panel for
/// a subagent while it runs. It owns the message list element and scrolls to
/// the bottom when the active session changes.

import { createEffect, Index, Show, type Accessor } from "solid-js";
import type { ChatMessage, SubagentData } from "./types";

export function MessageList(props: {
  hasProject: Accessor<boolean>;
  activeSessionId: Accessor<string | null>;
  messages: Accessor<ChatMessage[]>;
  liveSubagent: Accessor<SubagentData | null>;
}) {
  let messagesEl: HTMLUListElement | undefined;

  // Scroll to the bottom when the active session changes (opening a stored
  // chat or switching between chats). Only tracks `activeSessionId`, so it
  // does not re-run on every streaming chunk. `requestAnimationFrame` ensures
  // the new messages are painted before we measure `scrollHeight`.
  createEffect(() => {
    if (props.activeSessionId() && messagesEl) {
      requestAnimationFrame(() => {
        if (messagesEl) {
          messagesEl.scrollTop = messagesEl.scrollHeight;
        }
      });
    }
  });

  return (
    <ul class="chat-messages" ref={(el) => (messagesEl = el)}>
      <Show when={!props.hasProject()}>
        <li class="chat-empty">
          Open a folder to start chatting. Chats are saved per project.
        </li>
      </Show>
      <Show when={props.hasProject() && props.messages().length === 0}>
        <li class="chat-empty">
          Start a conversation — your chats are saved and can be reopened.
        </li>
      </Show>
      {/* Keyed on the session so switching chats builds a fresh list
          rather than reusing the previous session's rows (and their
          expanded/collapsed thinking panels) by index. */}
      <Show when={props.activeSessionId()} keyed>
        {/* <Index> keys by position, so a streaming message reuses its
            existing <li> and only its text nodes change. A plain .map()
            returns all-new DOM nodes, which Solid reconciles by node
            identity — it would replace every row on every chunk, throwing
            away the <details> element (and any click on its summary)
            several times a second. */}
        <Index each={props.messages()}>
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
      <Show when={props.liveSubagent()} keyed>
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
                        <span class="chat-toolcall-result">{stc().result}</span>
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
  );
}
