/// The file browser: the left "Files" pane (a recursive tree of the open
/// project) and the center editor pane (syntax-highlighted text or an image
/// preview). It owns all file-scoped state — the tree, the selected file, its
/// contents, the save status, and the editor zoom — and reacts to the open
/// root (passed in from App) by loading the tree and resetting file state.
///
/// The root itself is owned by App because it is shared with the toolbar and
/// the chat panel; this component only reads it and reports the persisted root
/// back on mount via `onRootChange`.

import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
  type Accessor,
} from "solid-js";
import { FileIcon } from "./FileIcon";
import { detectLang, highlight } from "../lib/highlight";
import { startPaneResize } from "../lib/resize";
import {
  getRoot,
  listTree,
  readFile,
  readFileData,
  writeFile,
  type RecentRoot,
  type TreeNode,
} from "../lib/ipc";

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

export function FileBrowser(props: {
  rootPath: Accessor<string>;
  recent: Accessor<RecentRoot[]>;
  onOpenRecent: (path: string) => void;
  onRootChange: (root: string) => void;
}) {
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
  const [filesWidth, setFilesWidth] = createSignal(22);
  // Editor zoom as a percentage (100 = default). Both the highlight layer and
  // the textarea size in em, so scaling the editor's font size keeps them
  // aligned. Adjusted with Ctrl + mouse wheel.
  const [editorZoom, setEditorZoom] = createSignal(100);

  /// Highlighted HTML for the current file, recomputed only when the content
  /// or the selected file changes.
  const highlighted = createMemo(() =>
    highlight(fileContent(), detectLang(selectedPath())),
  );

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let highlightEl: HTMLPreElement | undefined;
  let editorEl: HTMLDivElement | undefined;

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
    const root = props.rootPath();
    if (!path || !root || selectedIsImage()) {
      return;
    }
    setSaveState("saving");
    saveTimer = setTimeout(async () => {
      saveTimer = undefined;
      // If the root changed (or was closed) since this save was scheduled,
      // the edit belongs to a previous session — writing it now could land
      // in a different folder, so discard it.
      if (props.rootPath() !== root) {
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

  /// Zooms the editor with Ctrl + mouse wheel. Must be a non-passive listener
  /// so we can preventDefault the browser's own page zoom.
  function onEditorWheel(e: WheelEvent) {
    if (!e.ctrlKey) {
      return;
    }
    e.preventDefault();
    const step = e.deltaY < 0 ? 10 : -10;
    setEditorZoom((z) => Math.min(300, Math.max(50, z + step)));
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

  // Load the persisted root on mount and report it to App (which shares it
  // with the toolbar and the chat panel). If it can't be determined, surface
  // the error in the files pane and leave the no-folder state as-is.
  onMount(async () => {
    try {
      const root = await getRoot();
      props.onRootChange(root ?? "");
    } catch (err) {
      setTreeError(String(err));
    }
  });

  // Whenever the open root changes, reset file state and load the tree (or
  // clear it when no folder is open). This replaces the explicit load/reset
  // calls that used to live in App's open/close/recent handlers.
  createEffect(() => {
    const root = props.rootPath();
    if (!root) {
      resetFileState();
      setTree([]);
      setTreeError("");
      return;
    }
    resetFileState();
    void loadTree();
  });

  return (
    <div class="work-area">
      <section class="files" style={{ "flex-basis": `${filesWidth()}em` }}>
        <Show
          when={props.rootPath()}
          fallback={
            <div class="recent">
              <p class="content-placeholder">No folder open.</p>
              <Show when={props.recent().length > 0}>
                <h3>Recent</h3>
                <ul class="recent-list">
                  {props.recent().map((r) => (
                    <li>
                      <button
                        type="button"
                        class="recent-item"
                        title={r.path}
                        onClick={() => props.onOpenRecent(r.path)}
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
        onPointerDown={(e) =>
          startPaneResize(
            e,
            { get: filesWidth, set: setFilesWidth },
            {
              minEm: 12,
              maxEm: 80,
            },
          )
        }
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
                <div
                  class="editor"
                  ref={(el) => {
                    // Attach a non-passive wheel listener so Ctrl+wheel can
                    // preventDefault the browser's page zoom. The editor is
                    // conditionally rendered, so this runs when it appears
                    // (el) and is removed (null).
                    editorEl?.removeEventListener("wheel", onEditorWheel);
                    editorEl = el;
                    el?.addEventListener("wheel", onEditorWheel, {
                      passive: false,
                    });
                  }}
                  style={{
                    "font-size": `${editorZoom()}%`,
                  }}
                >
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
                        highlightEl.scrollLeft = e.currentTarget.scrollLeft;
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
  );
}
