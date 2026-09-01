/// The model combobox: a text input that filters a list of models grouped by
/// provider, with full keyboard navigation. It owns its own state (the fetched
/// model list, open/close, filter query, active option) and reports the
/// currently selected model up to the parent via `onSelectedModel`, so the
/// parent can drive context-window math and send requests without owning the
/// picker's internals.

import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  Show,
  type Accessor,
  type Setter,
} from "solid-js";
import { listModels, type Model, type Provider } from "../../lib/ipc";
import { FALLBACK_MODELS, type ModelGroup } from "./types";

export function ModelPicker(props: {
  modelId: Accessor<string>;
  setModelId: Setter<string>;
  setModelChosen: (v: boolean) => void;
  provider: Accessor<Provider>;
  keyMasked: Accessor<string>;
  baseUrl: Accessor<string>;
  onSelectedModel: (m: Model) => void;
}) {
  const [models, setModels] = createSignal<Model[]>(FALLBACK_MODELS);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(0);
  // The text in the combobox input. When the picker is closed it mirrors the
  // selected model's name; while open it is the user's (case-insensitive)
  // filter query.
  const [query, setQuery] = createSignal("");
  const [modelsLoading, setModelsLoading] = createSignal(false);
  const [modelsError, setModelsError] = createSignal("");

  let pickerEl: HTMLDivElement | undefined;
  let menuEl: HTMLUListElement | undefined;
  let inputEl: HTMLInputElement | undefined;

  /// The currently selected model object (falls back to the first if the id
  /// is not in the list, e.g. after a model list refresh).
  function selectedModel(): Model {
    return models().find((m) => m.id === props.modelId()) ?? models()[0];
  }

  // Report the resolved selected model to the parent whenever it changes.
  createEffect(() => {
    props.onSelectedModel(selectedModel());
  });

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

  // Load the real models for the active provider. Runs on mount and again when
  // the provider, base URL, or key changes (e.g. from the Settings page), so
  // the picker stays in sync without the panel being remounted. OpenRouter
  // needs a stored key; llama.cpp does not.
  createEffect(() => {
    // Reading `baseUrl()` here (even though it isn't used in the condition)
    // makes the effect re-run when the llama.cpp server address changes.
    props.baseUrl();
    if (props.provider() === "llama-cpp" || props.keyMasked()) {
      void loadModels();
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

  // Close the combobox when a click lands outside of it.
  function onDocClick(e: MouseEvent) {
    if (pickerEl && !pickerEl.contains(e.target as Node)) {
      closePicker();
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <>
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
      <Show when={modelsLoading()}>
        <span class="key-note">Loading models…</span>
      </Show>
      <Show when={modelsError() && !modelsLoading()}>
        <span class="key-note error" title={modelsError()}>
          Using fallback models
        </span>
      </Show>
    </>
  );
}
