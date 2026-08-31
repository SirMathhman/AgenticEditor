/// Inline SVG icons for file types. Each icon is a small, self-contained
/// vector (16x16 viewBox, sized to 1em so it scales with the tree font).
/// No external assets or dependencies — the shapes are drawn by hand so the
/// app stays dependency-free and the icons always render.
///
/// Note: SolidJS uses kebab-case SVG attributes (stroke-width, not
/// strokeWidth), so all SVG props below are written in kebab-case.

import type { JSX } from "solid-js";

type IconProps = { name: string; isDir: boolean; open?: boolean };
type Glyph = JSX.Element;

// A folder, tinted amber. `open` flips the front panel to a slightly lighter
// shade so expanded folders read as "open".
function FolderIcon({ open }: { open: boolean }) {
  return (
    <svg viewBox="0 0 16 16" width="1em" height="1em" aria-hidden="true">
      <path
        d="M1.5 3.5c0-.55.45-1 1-1h3.1c.28 0 .55.11.75.31l1.06 1.06c.2.2.47.31.75.31h5.34c.55 0 1 .45 1 1v6.32c0 .55-.45 1-1 1h-11c-.55 0-1-.45-1-1V3.5z"
        fill="#e8b04b"
      />
      <path
        d="M1.5 6.2h13l-1.1 5.2c-.1.45-.5.78-.96.78H3.56c-.46 0-.86-.33-.96-.78L1.5 6.2z"
        fill={open ? "#f2c56b" : "#d99f38"}
      />
    </svg>
  );
}

// A document with a folded corner and a small glyph drawn on top, used for
// code/config types.
function GlyphDoc({ color, glyph }: { color: string; glyph: Glyph }) {
  return (
    <svg viewBox="0 0 16 16" width="1em" height="1em" aria-hidden="true">
      <path
        d="M4 1.5h5.5L13 5v9.5c0 .28-.22.5-.5.5h-8c-.28 0-.5-.22-.5-.5v-12c0-.28.22-.5.5-.5z"
        fill={color}
        opacity="0.16"
      />
      <path
        d="M4 1.5h5.5L13 5v9.5c0 .28-.22.5-.5.5h-8c-.28 0-.5-.22-.5-.5v-12c0-.22.22-.5.5-.5z"
        fill="none"
        stroke={color}
        stroke-width="1.1"
        stroke-linejoin="round"
      />
      <path
        d="M9.5 1.5V5H13"
        fill="none"
        stroke={color}
        stroke-width="1.1"
        stroke-linejoin="round"
      />
      {glyph}
    </svg>
  );
}

// Angle brackets </> for generic code.
function CodeGlyph({ color }: { color: string }) {
  return (
    <g
      stroke={color}
      stroke-width="1.2"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M6 6.5 4 8l2 1.5" />
      <path d="M10 6.5 12 8l-2 1.5" />
    </g>
  );
}

// Curly braces for JSON.
function BracesGlyph({ color }: { color: string }) {
  return (
    <g
      stroke={color}
      stroke-width="1.2"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M6.5 5.5c-.8 0-.8.7-.8 1.2 0 .5-.2.8-.7.8.5 0 .7.3.7.8 0 .5 0 1.2.8 1.2" />
      <path d="M9.5 5.5c.8 0 .8.7.8 1.2 0 .5.2.8.7.8-.5 0-.7.3-.7.8 0 .5 0 1.2-.8 1.2" />
    </g>
  );
}

// A paint drop for CSS.
function DropGlyph({ color }: { color: string }) {
  return (
    <path
      d="M8 4.5c1.6 1.9 2.4 3.1 2.4 4.2A2.4 2.4 0 0 1 8 11.1a2.4 2.4 0 0 1-2.4-2.4C5.6 7.6 6.4 6.4 8 4.5z"
      fill={color}
    />
  );
}

// A simple "M" for Markdown.
function MarkdownGlyph({ color }: { color: string }) {
  return (
    <path
      d="M4.5 10.5v-5h1.2l1.3 1.8 1.3-1.8h1.2v5H8.3V7.6L7 9.2 5.7 7.6v2.9H4.5z"
      fill={color}
    />
  );
}

// A gear for config files (TOML/YAML).
function GearGlyph({ color }: { color: string }) {
  return (
    <g fill={color}>
      <circle cx="8" cy="8" r="1.4" />
      <path
        d="M8 4.2l.5 1.2 1.3-.3.2 1.3 1.2.5-1 .9 1 .9-1.2.5-.2 1.3-1.3-.3-.5 1.2-.5-1.2-1.3.3-.2-1.3-1.2-.5 1-.9-1-.9 1.2-.5.2-1.3 1.3.3z"
        opacity="0.9"
      />
    </g>
  );
}

// A padlock for lockfiles.
function LockGlyph({ color }: { color: string }) {
  return (
    <g fill={color}>
      <rect x="5" y="7.5" width="6" height="4" rx="0.8" />
      <path
        d="M6 7.5V6a2 2 0 0 1 4 0v1.5"
        fill="none"
        stroke={color}
        stroke-width="1.1"
      />
    </g>
  );
}

// Horizontal lines for plain text.
function TextGlyph({ color }: { color: string }) {
  return (
    <g stroke={color} stroke-width="1.1" stroke-linecap="round">
      <path d="M5 6h6" />
      <path d="M5 8h6" />
      <path d="M5 10h4" />
    </g>
  );
}

// A git branch for .gitignore / .gitattributes.
function GitGlyph({ color }: { color: string }) {
  return (
    <g stroke={color} stroke-width="1.1" fill="none" stroke-linecap="round">
      <circle cx="5.5" cy="5.5" r="1.1" />
      <circle cx="5.5" cy="10.5" r="1.1" />
      <circle cx="10.5" cy="6.5" r="1.1" />
      <path d="M5.5 6.6v2.8" />
      <path d="M10.5 7.6c0 1.6-2 1.4-3.5 2.2" />
    </g>
  );
}

// A picture (frame + sun + hill) for images.
function ImageIcon() {
  return (
    <svg viewBox="0 0 16 16" width="1em" height="1em" aria-hidden="true">
      <rect x="2" y="3" width="12" height="10" rx="1.2" fill="#4f9d6b" opacity="0.18" />
      <rect
        x="2"
        y="3"
        width="12"
        height="10"
        rx="1.2"
        fill="none"
        stroke="#4f9d6b"
        stroke-width="1.1"
      />
      <circle cx="5.5" cy="6.2" r="1.1" fill="#4f9d6b" />
      <path
        d="M3 11.5 6.5 8l2.5 2.2 1.8-1.5 2.2 2.3"
        fill="none"
        stroke="#4f9d6b"
        stroke-width="1.1"
        stroke-linejoin="round"
      />
    </svg>
  );
}

// A terminal prompt for shell scripts.
function ShellGlyph({ color }: { color: string }) {
  return (
    <g
      stroke={color}
      stroke-width="1.2"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M5 6 7.5 8 5 10" />
      <path d="M8.5 10.5h3" />
    </g>
  );
}

// A database cylinder for data files.
function DbGlyph({ color }: { color: string }) {
  return (
    <g fill="none" stroke={color} stroke-width="1.1">
      <ellipse cx="8" cy="5.5" rx="3" ry="1.2" />
      <path d="M5 5.5v5c0 .66 1.34 1.2 3 1.2s3-.54 3-1.2v-5" />
      <path d="M5 8c0 .66 1.34 1.2 3 1.2S11 8.66 11 8" />
    </g>
  );
}

// A music note for audio.
function MusicGlyph({ color }: { color: string }) {
  return (
    <g fill={color}>
      <path d="M10.5 4.5 6.5 5.4v4.1a1.4 1.4 0 1 1-.7-1.2V6.4l4.7-.9v-1z" />
    </g>
  );
}

// A film frame for video.
function FilmGlyph({ color }: { color: string }) {
  return (
    <g fill="none" stroke={color} stroke-width="1.1">
      <rect x="3.5" y="4.5" width="9" height="7" rx="1" />
      <path d="M6 4.5v7M10 4.5v7" />
    </g>
  );
}

// A zip box for archives.
function ZipGlyph({ color }: { color: string }) {
  return (
    <g fill={color}>
      <rect x="4.5" y="4" width="7" height="8" rx="0.8" opacity="0.25" />
      <rect
        x="4.5"
        y="4"
        width="7"
        height="8"
        rx="0.8"
        fill="none"
        stroke={color}
        stroke-width="1.1"
      />
      <path d="M8 4v8" stroke={color} stroke-width="1.1" />
      <path d="M7 5.5h2M7 7h2M7 8.5h2" stroke={color} stroke-width="0.9" />
    </g>
  );
}

// A binary/byte icon for unknown binary files.
function BinaryGlyph({ color }: { color: string }) {
  return (
    <g fill={color}>
      <rect x="4" y="4.5" width="8" height="7" rx="1" opacity="0.2" />
      <rect
        x="4"
        y="4.5"
        width="8"
        height="7"
        rx="1"
        fill="none"
        stroke={color}
        stroke-width="1.1"
      />
      <path d="M6 7h1.5M8.5 7H10M6 9h1.5M8.5 9H10" stroke={color} stroke-width="1" />
    </g>
  );
}

// Brand-ish colors per language, chosen to be distinct and readable on both
// light and dark surfaces.
const COLORS = {
  code: "#5b9bd5",
  rust: "#dea584",
  python: "#4b8bbe",
  css: "#a06cd5",
  json: "#d9a441",
  html: "#e07b54",
  markdown: "#6a9955",
  config: "#8a8a8a",
  lock: "#c9a227",
  text: "#9aa0a6",
  git: "#f05033",
  shell: "#4ec9b0",
  data: "#c586c0",
  audio: "#ce9178",
  video: "#c586c0",
  archive: "#d9a441",
  binary: "#8a8a8a",
};

// Map a file extension (lowercase, no dot) to a color + glyph.
function classify(ext: string): { color: string; glyph: Glyph } {
  switch (ext) {
    case "rs":
      return { color: COLORS.rust, glyph: <CodeGlyph color={COLORS.rust} /> };
    case "ts":
    case "tsx":
    case "mts":
    case "cts":
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return { color: COLORS.code, glyph: <CodeGlyph color={COLORS.code} /> };
    case "py":
      return { color: COLORS.python, glyph: <CodeGlyph color={COLORS.python} /> };
    case "css":
    case "scss":
    case "less":
      return { color: COLORS.css, glyph: <DropGlyph color={COLORS.css} /> };
    case "json":
    case "jsonc":
      return { color: COLORS.json, glyph: <BracesGlyph color={COLORS.json} /> };
    case "html":
    case "htm":
      return { color: COLORS.html, glyph: <CodeGlyph color={COLORS.html} /> };
    case "md":
    case "mdx":
      return { color: COLORS.markdown, glyph: <MarkdownGlyph color={COLORS.markdown} /> };
    case "toml":
    case "yaml":
    case "yml":
    case "ini":
    case "conf":
      return { color: COLORS.config, glyph: <GearGlyph color={COLORS.config} /> };
    case "lock":
      return { color: COLORS.lock, glyph: <LockGlyph color={COLORS.lock} /> };
    case "txt":
    case "log":
    case "text":
      return { color: COLORS.text, glyph: <TextGlyph color={COLORS.text} /> };
    case "gitignore":
    case "gitattributes":
    case "gitmodules":
      return { color: COLORS.git, glyph: <GitGlyph color={COLORS.git} /> };
    case "sh":
    case "bash":
    case "ps1":
    case "bat":
    case "cmd":
      return { color: COLORS.shell, glyph: <ShellGlyph color={COLORS.shell} /> };
    case "sql":
    case "db":
    case "sqlite":
      return { color: COLORS.data, glyph: <DbGlyph color={COLORS.data} /> };
    case "mp3":
    case "wav":
    case "flac":
    case "ogg":
      return { color: COLORS.audio, glyph: <MusicGlyph color={COLORS.audio} /> };
    case "mp4":
    case "webm":
    case "mov":
    case "mkv":
      return { color: COLORS.video, glyph: <FilmGlyph color={COLORS.video} /> };
    case "zip":
    case "tar":
    case "gz":
    case "7z":
    case "rar":
      return { color: COLORS.archive, glyph: <ZipGlyph color={COLORS.archive} /> };
    default:
      return { color: COLORS.binary, glyph: <BinaryGlyph color={COLORS.binary} /> };
  }
}

// Image extensions get the picture icon; everything else is classified by
// extension. Dotfiles (e.g. `.gitignore`) are matched on their full name.
const IMAGE_EXTS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "svg",
  "webp",
  "bmp",
  "ico",
]);

export function FileIcon(props: IconProps) {
  if (props.isDir) {
    return <FolderIcon open={props.open ?? false} />;
  }
  const name = props.name.toLowerCase();
  const dot = name.lastIndexOf(".");
  const ext = dot >= 0 ? name.slice(dot + 1) : "";
  if (IMAGE_EXTS.has(ext)) {
    return <ImageIcon />;
  }
  // Dotfiles: classify on the name without the leading dot.
  const key = name.startsWith(".") && dot === 0 ? name.slice(1) : ext;
  const { color, glyph } = classify(key);
  return <GlyphDoc color={color} glyph={glyph} />;
}
