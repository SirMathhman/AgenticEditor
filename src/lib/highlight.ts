/// Lightweight, regex-based syntax highlighting. No lexing or parsing — just
/// token categories (keyword, string, comment, number, type) matched per
/// language, in the spirit of IntelliJ's token-color groups.

export type TokenCategory =
  "keyword" | "string" | "comment" | "number" | "type";

interface Rule {
  /// Regex source. Must not contain capturing groups (use `(?:...)`).
  re: string;
  cat: TokenCategory;
}

interface Lang {
  rules: Rule[];
}

/// Escape a string for safe insertion into HTML.
function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

const KW = {
  rust: "fn|let|mut|const|if|else|match|for|while|loop|return|break|continue|use|mod|pub|struct|enum|impl|trait|type|where|async|await|move|ref|self|Self|super|crate|dyn|unsafe|extern|static|as|in|true|false|Some|None|Ok|Err",
  ts: "const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|new|class|extends|super|import|export|from|default|try|catch|finally|throw|async|await|yield|typeof|instanceof|in|of|delete|void|this|true|false|null|undefined|interface|type|enum|implements|public|private|protected|readonly|static|get|set|as|satisfies|keyof|infer|never|any|unknown|number|string|boolean|object|symbol|bigint",
  python:
    "def|class|return|if|elif|else|for|while|break|continue|pass|import|from|as|try|except|finally|raise|with|lambda|yield|global|nonlocal|assert|del|in|is|not|and|or|True|False|None|self|async|await",
};

const LANGS: Record<string, Lang> = {
  rust: {
    rules: [
      { re: "//.*|/\\*[\\s\\S]*?\\*/", cat: "comment" },
      // Double-quoted strings (no newlines) and char literals (exactly one
      // char or an escape sequence between the quotes).
      {
        re: `"(?:\\\\.|[^"\\\\\\n])*"|'(?:\\\\.|[^'\\\\\\n])'`,
        cat: "string",
      },
      { re: `\\b(?:${KW.rust})\\b`, cat: "keyword" },
      { re: "\\b[A-Z][A-Za-z0-9_]*\\b", cat: "type" },
      {
        re: "\\b\\d[\\d_]*(?:\\.\\d+)?(?:[fF]64|[fF]32|[uU]\\d+)?\\b",
        cat: "number",
      },
    ],
  },
  ts: {
    rules: [
      { re: "//.*|/\\*[\\s\\S]*?\\*/", cat: "comment" },
      {
        re: `"(?:\\\\.|[^"\\\\\\n])*"|'(?:\\\\.|[^'\\\\\\n])*'|\`(?:\\\\.|[^\`\\\\])*\``,
        cat: "string",
      },
      { re: `\\b(?:${KW.ts})\\b`, cat: "keyword" },
      { re: "\\b[A-Z][A-Za-z0-9_]*\\b", cat: "type" },
      { re: "\\b\\d[\\d_]*(?:\\.\\d+)?\\b", cat: "number" },
    ],
  },
  python: {
    rules: [
      { re: "#.*", cat: "comment" },
      {
        re: `"""[\\s\\S]*?"""|'''[\\s\\S]*?'''|"(?:\\\\.|[^"\\\\\\n])*"|'(?:\\\\.|[^'\\\\\\n])*'`,
        cat: "string",
      },
      { re: `\\b(?:${KW.python})\\b`, cat: "keyword" },
      { re: "\\b[A-Z][A-Za-z0-9_]*\\b", cat: "type" },
      { re: "\\b\\d[\\d_]*(?:\\.\\d+)?\\b", cat: "number" },
    ],
  },
  css: {
    rules: [
      { re: "/\\*[\\s\\S]*?\\*/", cat: "comment" },
      { re: `"(?:\\\\.|[^"\\\\])*"|'(?:\\\\.|[^'\\\\])*'`, cat: "string" },
      { re: "@[\\w-]+", cat: "keyword" },
      { re: "[\\w-]+(?=\\s*:)", cat: "keyword" },
      { re: "[.#][\\w-]+", cat: "type" },
      {
        re: "#[0-9a-fA-F]{3,8}\\b|\\b\\d+(?:\\.\\d+)?(?:px|em|rem|%|vh|vw|s|ms|deg)?\\b",
        cat: "number",
      },
    ],
  },
  json: {
    rules: [
      { re: `"(?:\\\\.|[^"\\\\])*"`, cat: "string" },
      { re: "\\b(?:true|false|null)\\b", cat: "keyword" },
      { re: "-?\\b\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?\\b", cat: "number" },
    ],
  },
  html: {
    rules: [
      { re: "<!--[\\s\\S]*?-->", cat: "comment" },
      { re: `"[^"]*"|'[^']*'`, cat: "string" },
      { re: "</?[\\w-]+|/?>", cat: "keyword" },
      { re: "[\\w-]+(?==)", cat: "type" },
    ],
  },
  /// Minimal fallback for unknown languages: strings, line comments, numbers.
  plain: {
    rules: [
      { re: "//.*|#[^!].*|/\\*[\\s\\S]*?\\*/", cat: "comment" },
      { re: `"(?:\\\\.|[^"\\\\])*"|'(?:\\\\.|[^'\\\\])*'`, cat: "string" },
      { re: "\\b\\d[\\d_]*(?:\\.\\d+)?\\b", cat: "number" },
    ],
  },
};

const EXT_TO_LANG: Record<string, string> = {
  rs: "rust",
  ts: "ts",
  tsx: "ts",
  mts: "ts",
  cts: "ts",
  js: "ts",
  jsx: "ts",
  mjs: "ts",
  cjs: "ts",
  py: "python",
  css: "css",
  json: "json",
  html: "html",
  htm: "html",
};

/// Detects a language id from a file path's extension. Returns `undefined`
/// when the extension is unknown (callers fall back to plain highlighting).
export function detectLang(path: string): string | undefined {
  const dot = path.lastIndexOf(".");
  if (dot < 0) return undefined;
  const ext = path.slice(dot + 1).toLowerCase();
  return EXT_TO_LANG[ext];
}

/// Files larger than this (in characters) are not highlighted, to avoid
/// blocking the main thread with a large regex scan + HTML rebuild.
const MAX_HIGHLIGHT_CHARS = 50_000;

/// Returns an HTML string with token spans for `code`, highlighted for `lang`
/// (or the plain fallback when `lang` is unknown). The output is safe to set
/// as `innerHTML`. Very large files are returned escaped but unhighlighted.
export function highlight(code: string, lang?: string): string {
  if (code.length > MAX_HIGHLIGHT_CHARS) {
    return escapeHtml(code);
  }
  const def = (lang && LANGS[lang]) || LANGS.plain;
  const combined = new RegExp(def.rules.map((r) => `(${r.re})`).join("|"), "g");
  let out = "";
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = combined.exec(code)) !== null) {
    if (m.index > last) out += escapeHtml(code.slice(last, m.index));
    let cat: TokenCategory | null = null;
    for (let i = 1; i < m.length; i++) {
      if (m[i] !== undefined) {
        cat = def.rules[i - 1].cat;
        break;
      }
    }
    const text = escapeHtml(m[0]);
    out += cat ? `<span class="tok-${cat}">${text}</span>` : text;
    last = m.index + m[0].length;
    if (m[0].length === 0) combined.lastIndex++;
  }
  out += escapeHtml(code.slice(last));
  return out;
}
