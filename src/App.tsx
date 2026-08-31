import { createSignal, onCleanup, onMount, Show } from "solid-js";
import {
  getRoot,
  isImagePath,
  listTree,
  pickRootFolder,
  readFile,
  readFileData,
  writeFile,
  type TreeNode,
} from "./lib/ipc";
import "./App.css";

function TreeItem(props: {
  node: TreeNode;
  selectedPath: string;
  onSelect: (path: string) => void;
}) {
  const [expanded, setExpanded] = createSignal(false);

  function handleClick() {
    if (props.node.is_dir) {
      setExpanded(!expanded());
    } else {
      props.onSelect(props.node.path);
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
        }}
        onClick={handleClick}
      >
        <span class="tree-caret">
          {props.node.is_dir ? (expanded() ? "▾" : "▸") : "·"}
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
  const [fileContent, setFileContent] = createSignal("");
  const [imageSrc, setImageSrc] = createSignal("");
  const [fileError, setFileError] = createSignal("");
  const [saveState, setSaveState] = createSignal<
    "idle" | "saving" | "saved" | "error"
  >("idle");
  const [rootPath, setRootPath] = createSignal("");

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  async function openFolder() {
    try {
      const newRoot = await pickRootFolder(rootPath() || undefined);
      if (newRoot === null) {
        return; // user cancelled
      }
      setRootPath(newRoot);
      setSelectedPath("");
      setFileContent("");
      setImageSrc("");
      setFileError("");
      setSaveState("idle");
      await loadTree();
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
    if (!path || isImagePath(path)) {
      return;
    }
    setSaveState("saving");
    saveTimer = setTimeout(async () => {
      saveTimer = undefined;
      try {
        await writeFile(path, content);
        setSaveState("saved");
      } catch (err) {
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
      setTreeError(String(err));
    }
  }

  async function selectFile(path: string) {
    clearSaveTimer();
    setSelectedPath(path);
    setFileError("");
    setFileContent("");
    setImageSrc("");
    setSaveState("idle");
    try {
      if (isImagePath(path)) {
        const data = await readFileData(path);
        setImageSrc(`data:${data.mime_type};base64,${data.data}`);
      } else {
        setFileContent(await readFile(path));
      }
    } catch (err) {
      setFileError(String(err));
    }
  }

  onMount(async () => {
    try {
      setRootPath(await getRoot());
    } catch {
      // Root display is non-critical; the tree still loads below.
    }
    loadTree();
  });

  return (
    <main class="container">
      <nav class="toolbar">
        <button type="button" onClick={openFolder}>
          File
        </button>
        <span class="toolbar-root" title={rootPath()}>
          {rootPath()}
        </span>
      </nav>
      <div class="main-row">
        <section class="files">
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
        </section>

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
                  <textarea
                    class="content-textarea"
                    value={fileContent()}
                    onInput={(e) => {
                      setFileContent(e.currentTarget.value);
                      scheduleSave();
                    }}
                    spellcheck={false}
                  />
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
    </main>
  );
}

export default App;
