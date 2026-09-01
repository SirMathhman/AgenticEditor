/// The session (chat) picker in the chat header: a toggle showing the active
/// chat's title and a menu to switch chats, start a new one, or delete one.
/// Owns its open state and closes on outside click.

import { createSignal, onCleanup, Show, type Accessor } from "solid-js";
import type { ChatSession } from "../../lib/ipc";

export function SessionPicker(props: {
  hasProject: Accessor<boolean>;
  sessions: Accessor<ChatSession[]>;
  activeSessionId: Accessor<string | null>;
  setActiveSessionId: (id: string | null) => void;
  newSession: () => void;
  deleteSession: (id: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  let el: HTMLDivElement | undefined;

  function onDocClick(e: MouseEvent) {
    if (el && !el.contains(e.target as Node) && open()) {
      setOpen(false);
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <div class="session-picker" ref={(e) => (el = e)}>
      <button
        type="button"
        class="session-toggle"
        disabled={!props.hasProject()}
        onClick={() => setOpen((v) => !v)}
      >
        <span class="session-title">
          {props.sessions().find((s) => s.id === props.activeSessionId())
            ?.title ?? "New chat"}
        </span>
        <span class="session-caret">▾</span>
      </button>
      <Show when={open()}>
        <div class="session-menu">
          <button type="button" class="session-new" onClick={props.newSession}>
            + New chat
          </button>
          <ul>
            {props.sessions().map((s) => (
              <li
                class="session-item"
                classList={{ active: s.id === props.activeSessionId() }}
                onClick={() => {
                  props.setActiveSessionId(s.id);
                  setOpen(false);
                }}
              >
                <span class="session-item-title">{s.title}</span>
                <button
                  type="button"
                  class="session-delete"
                  title="Delete chat"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.deleteSession(s.id);
                  }}
                >
                  ✕
                </button>
              </li>
            ))}
            <Show when={props.sessions().length === 0}>
              <li class="session-empty">No chats yet</li>
            </Show>
          </ul>
        </div>
      </Show>
    </div>
  );
}
