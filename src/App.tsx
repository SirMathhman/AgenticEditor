import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import { ChatPanel } from "./components/ChatPanel";
import { FileIcon } from "./components/FileIcon";
import { SettingsPage } from "./components/SettingsPage";
import { detectLang, highlight } from "./lib/highlight";
import {
  closeRoot,
  getKey,
  getRoot,
  listTree,
  pickRootFolder,
  readFile,
  readFileData,
  recentRoots,
  setRoot,
  writeFile,
  type RecentRoot,
  type TreeNode,
} from "./lib/ipc";
import "./App.css";

function TreeItem(props: {
  node: TreeNode;
  selectedPath: string;
  onSelect: (node: TreeNode) => void;
}) {
  const [expanded, setExpanded] = createSignal(false);

  /// Excluded dirs (e.g. node_modules, .git) are shown greyed out and are not
  /// expandable or selectable.
  const excluded = () => props.node.is_excluded;

  function handleClick() {
    if (excluded()) {
      return;
    }
    if (props.node.is_dir) {
      setExpanded(!expanded());
    } else {
      props.onSelect(props.node);
    }
  }

  return (
    <li class="tree-item">
      <button
        type="button"
        class="tree-row"
        classList={{
          selected:
            !props.node.is_dir && props.selectedPath === props.node.path,
          excluded: excluded(),
        }}
        // aria-disabled (not `disabled`) keeps the node in the accessibility
        // tree so screen readers announce it as non-interactive rather than
        // skipping it. The click handler is a no-op for excluded rows.
        aria-disabled={excluded() || undefined}
        onClick={handleClick}
      >
        <span class="tree-caret">
          {props.node.is_dir && !excluded() ? (expanded() ? "▾" : "▸") : ""}
        </span>
        <span class="tree-icon">
          <FileIcon
            name={props.node.name}
            isDir={props.node.is_dir}
            isImage={props.node.is_image}
            open={expanded()}
          />
        </span>
        <span class={props.node.is_dir ? "tree-dir" : "tree-file"}>
          {props.node.name}
        </span>
      </button>
      <Show when={props.node.is_dir && expanded() && props.node.children}>
        <ul class="tree-children">
          {props.node.children!.map((child) => (
            <TreeItem
              node={child}
              selectedPath={props.selectedPath}
              onSelect={props.onSelect}
            />
          ))}
        </ul>
      </Show>
    </li>
  );
}

function App() {
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [treeError, setTreeError] = createSignal("");
  const [selectedPath, setSelectedPath] = createSignal("");
  const [selectedIsImage, setSelectedIsImage] = createSignal(false);
  const [fileContent, setFileContent] = createSignal("");
  const [imageSrc, setImageSrc] = createSignal("");
  const [fileError, setFileError] = createSignal("");
  const [saveState, setSaveState] = createSignal<
    "idle" | "saving" | "saved" | "error"
  >("idle");
  const [rootPath, setRootPath] = createSignal("");
  const [recent, setRecent] = createSignal<RecentRoot[]>([]);
  const [filesWidth, setFilesWidth] = createSignal(22);
  const [view, setView] = createSignal<"main" | "settings">("main");
  // The masked OpenRouter key, owned here so the chat panel and settings page
  // share one source of truth and stay in sync when the key changes.
  const [keyMasked, setKeyMasked] = createSignal("");

  /// Highlighted HTML for the current file, recomputed only when the content
  /// or the selected file changes.
  const highlighted = createMemo(() =>
    highlight(fileContent(), detectLang(selectedPath())),
  );

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let highlightEl: HTMLPreElement | undefined;

  /// Drags the divider between the explorer and content panes to resize the
  /// explorer horizontally. The width is stored in em (relative to the root
  /// font size) and clamped so neither pane collapses.
  function startResize(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = filesWidth();
    const minEm = 12;
    const maxEm = 80;
    // Convert pixel deltas to em using the actual root font size, so the
    // resize stays correct if the base font size differs from 16px.
    const pxPerEm =
      parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;

    function onMove(ev: PointerEvent) {
      const deltaEm = (ev.clientX - startX) / pxPerEm;
      const next = Math.min(maxEm, Math.max(minEm, startWidth + deltaEm));
      setFilesWidth(next);
    }

    function onUp() {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      document.body.style.cursor = "";
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    document.body.style.cursor = "col-resize";
  }

  async function loadRecent() {
    try {
      setRecent(await recentRoots());
    } catch {
      setRecent([]);
    }
  }

  async function openRecent(path: string) {
    try {
      const newRoot = await setRoot(path);
      setRootPath(newRoot);
      resetFileState();
      await loadTree();
    } catch (err) {
      setTreeError(String(err));
    }
  }

  /// True when an error means the root changed (or was closed) while the
  /// operation was in flight. Such results are stale and must be discarded
  /// silently rather than surfaced to the user.
  function isStaleRootError(err: unknown): boolean {
    return String(err).includes("root changed while operation was in flight");
  }

  /// Resets all file-scoped state. Called when the root changes or is closed.
  function resetFileState() {
    clearSaveTimer();
    setSelectedPath("");
    setSelectedIsImage(false);
    setFileContent("");
    setImageSrc("");
    setFileError("");
    setSaveState("idle");
  }

  async function openFolder() {
    try {
      const newRoot = await pickRootFolder(rootPath() || undefined);
      if (newRoot === null) {
        return; // user cancelled
      }
      setRootPath(newRoot);
      resetFileState();
      await loadTree();
    } catch (err) {
      setTreeError(String(err));
    }
  }

  async function closeFolder() {
    try {
      await closeRoot();
      setRootPath("");
      resetFileState();
      setTree([]);
      setTreeError("");
      loadRecent();
    } catch (err) {
      setTreeError(String(err));
    }
  }

  function clearSaveTimer() {
    if (saveTimer !== undefined) {
      clearTimeout(saveTimer);
      saveTimer = undefined;
    }
  }

  function scheduleSave() {
    clearSaveTimer();
    // Capture the path and content now, so the save is bound to this edit
    // session. Reading the live signals when the timer fires could write the
    // wrong (or empty) content if the user switches files in the meantime.
    const path = selectedPath();
    const content = fileContent();
    const root = rootPath();
    if (!path || !root || selectedIsImage()) {
      return;
    }
    setSaveState("saving");
    saveTimer = setTimeout(async () => {
      saveTimer = undefined;
      // If the root changed (or was closed) since this save was scheduled,
      // the edit belongs to a previous session — writing it now could land
      // in a different folder, so discard it.
      if (rootPath() !== root) {
        setSaveState("idle");
        return;
      }
      try {
        await writeFile(path, content);
        setSaveState("saved");
      } catch (err) {
        if (isStaleRootError(err)) {
          setSaveState("idle");
          return;
        }
        setSaveState("error");
        setFileError(String(err));
      }
    }, 100);
  }

  function saveLabel() {
    switch (saveState()) {
      case "saving":
        return "Saving…";
      case "saved":
        return "Saved";
      case "error":
        return "Save failed";
      default:
        return "";
    }
  }

  onCleanup(clearSaveTimer);

  async function loadTree() {
    try {
      setTree(await listTree());
      setTreeError("");
    } catch (err) {
      if (isStaleRootError(err)) {
        return; // root changed while loading; a fresh load will follow
      }
      setTreeError(String(err));
    }
  }

  async function selectFile(node: TreeNode) {
    resetFileState();
    setSelectedPath(node.path);
    setSelectedIsImage(node.is_image);
    try {
      if (node.is_image) {
        const data = await readFileData(node.path);
        setImageSrc(`data:${data.mime_type};base64,${data.data}`);
      } else {
        setFileContent(await readFile(node.path));
      }
    } catch (err) {
      if (isStaleRootError(err)) {
        return; // root changed while loading; the file is no longer relevant
      }
      setFileError(String(err));
    }
  }

  onMount(async () => {
    loadRecent();
    try {
      setKeyMasked((await getKey()) ?? "");
    } catch {
      // No key stored; the empty value is already in place.
    }
    try {
      const root = await getRoot();
      setRootPath(root ?? "");
      if (root !== null) {
        loadTree();
      }
    } catch (err) {
      // No root could be determined; leave the no-folder state as-is.
      setTreeError(String(err));
    }
  });

  return (
    <main class="container">
      <nav class="toolbar">
        <button type="button" class="btn-primary" onClick={openFolder}>
          File
        </button>
        <Show when={rootPath()}>
          <button type="button" onClick={closeFolder}>
            Close
          </button>
        </Show>
        <span class="toolbar-root" title={rootPath()}>
          {rootPath() || "No folder open"}
        </span>
        <button
          type="button"
          class="toolbar-settings"
          onClick={() => setView("settings")}
        >
          Settings
        </button>
      </nav>
      <Show when={view() === "main"}>
        <div class="main-row">
          <div class="work-area">
            <section
              class="files"
              style={{ "flex-basis": `${filesWidth()}em` }}
            >
              <Show
                when={rootPath()}
                fallback={
                  <div class="recent">
                    <p class="content-placeholder">No folder open.</p>
                    <Show when={recent().length > 0}>
                      <h3>Recent</h3>
                      <ul class="recent-list">
                        {recent().map((r) => (
                          <li>
                            <button
                              type="button"
                              class="recent-item"
                              title={r.path}
                              onClick={() => openRecent(r.path)}
                            >
                              {r.path}
                            </button>
                          </li>
                        ))}
                      </ul>
                    </Show>
                  </div>
                }
              >
                <div class="row">
                  <h2>Files</h2>
                  <button type="button" onClick={loadTree}>
                    Refresh
                  </button>
                </div>
                {treeError() ? (
                  <p class="error">{treeError()}</p>
                ) : (
                  <ul class="tree">
                    {tree().map((node) => (
                      <TreeItem
                        node={node}
                        selectedPath={selectedPath()}
                        onSelect={selectFile}
                      />
                    ))}
                  </ul>
                )}
              </Show>
            </section>

            <div
              class="divider"
              role="separator"
              aria-orientation="vertical"
              onPointerDown={startResize}
            ></div>

            <section class="content">
              <Show
                when={selectedPath()}
                fallback={
                  <p class="content-placeholder">
                    Select a file to view its contents.
                  </p>
                }
              >
                <div class="row">
                  <h2>{selectedPath()}</h2>
                  <Show when={saveState() !== "idle"}>
                    <span class="save-state">{saveLabel()}</span>
                  </Show>
                </div>
                <Show
                  when={!fileError()}
                  fallback={<p class="error">{fileError()}</p>}
                >
                  <Show
                    when={imageSrc()}
                    fallback={
                      <div class="editor">
                        <pre
                          class="editor-highlight"
                          aria-hidden="true"
                          ref={(el) => {
                            highlightEl = el;
                          }}
                          innerHTML={highlighted()}
                        ></pre>
                        <textarea
                          class="content-textarea"
                          value={fileContent()}
                          onInput={(e) => {
                            setFileContent(e.currentTarget.value);
                            scheduleSave();
                          }}
                          onScroll={(e) => {
                            if (highlightEl) {
                              highlightEl.scrollTop = e.currentTarget.scrollTop;
                              highlightEl.scrollLeft =
                                e.currentTarget.scrollLeft;
                            }
                          }}
                          spellcheck={false}
                        ></textarea>
                      </div>
                    }
                  >
                    <img
                      class="content-image"
                      src={imageSrc()}
                      alt={selectedPath()}
                    />
                  </Show>
                </Show>
              </Show>
            </section>
          </div>

          <section class="chat-panel">
            <ChatPanel keyMasked={keyMasked} />
          </section>
        </div>
      </Show>
      <Show when={view() === "settings"}>
        <SettingsPage
          keyMasked={keyMasked}
          onKeyChange={setKeyMasked}
          onBack={() => setView("main")}
        />
      </Show>
    </main>
  );
}

export default App;
