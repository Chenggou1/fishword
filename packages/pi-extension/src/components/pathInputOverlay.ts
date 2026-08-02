import { accessSync, constants, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { Input, Key, matchesKey, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import type { Component, TUI } from "@earendil-works/pi-tui";
import { OverlayFrame, type OverlayTheme } from "./overlayFrame.ts";

export interface PathInputOverlayState {
  path: string;
  selectedIndex: number;
}

export interface PathInputSuggestion {
  value: string;
  label: string;
  directory: boolean;
}

export interface PathInputOverlayOptions {
  title: string;
  label?: string;
  emptyMessage?: string;
  footer?: string;
  directoryLabel?: string;
  fileLabel?: string;
  suggestionLimit?: number;
  fileExtensions?: string[];
}

export type PathInputOverlayResult = string | undefined;

/**
 * Adapted from Chenggou1/FishRead's
 * packages/pi-extension/src/components/path-input-overlay.ts. The
 * implementation is kept local so Fishword can evolve its own import flow and
 * Boss-key lifecycle without a runtime dependency on the sibling project.
 */
export function normalizePathInput(input: string): string {
  const path = cleanPathInput(input);
  if (path === "~") return homedir();
  if (path.startsWith("~/") || path.startsWith("~\\")) {
    return `${homedir()}${path.slice(1)}`;
  }
  return path;
}

function cleanPathInput(input: string): string {
  let path = input.trim();
  const quoted =
    (path.startsWith('"') && path.endsWith('"')) ||
    (path.startsWith("'") && path.endsWith("'"));
  if (quoted && path.length >= 2) {
    path = path.slice(1, -1).trim();
  }

  return path;
}

export class PathInputOverlay implements Component {
  private readonly input = new Input();
  private suggestions: PathInputSuggestion[] = [];
  private selectedIndex = 0;
  private error: string | undefined;

  constructor(
    private theme: OverlayTheme,
    private tui: TUI,
    private done: (result: PathInputOverlayResult) => void,
    private options: PathInputOverlayOptions,
    initialState?: PathInputOverlayState,
  ) {
    this.input.setValue(initialState?.path ?? "");
    this.selectedIndex = initialState?.selectedIndex ?? 0;
    this.input.onSubmit = (value) => this.submit(value);
    this.input.onEscape = () => this.done(undefined);
    this.refreshSuggestions();
  }

  handleInput(data: string): void {
    if (matchesKey(data, Key.up)) {
      this.moveSelection(-1);
      return;
    }
    if (matchesKey(data, Key.down)) {
      this.moveSelection(1);
      return;
    }
    if (matchesKey(data, Key.tab)) {
      this.completeSelection();
      return;
    }
    if (matchesKey(data, Key.enter)) {
      const selected = this.selectedSuggestion();
      if (selected?.directory) {
        this.acceptSuggestion(selected);
        return;
      }
      if (selected) {
        this.submit(selected.value);
        return;
      }
    }

    this.error = undefined;
    this.input.handleInput(data);
    this.refreshSuggestions();
    this.tui.requestRender();
  }

  render(width: number): string[] {
    const contentWidth = Math.max(1, width - 2);
    const label = `${this.options.label ?? "路径"} `;
    const inputWidth = Math.max(1, contentWidth - visibleWidth(label));
    const inputLine = this.input.render(inputWidth)[0] ?? "";
    const limit = this.suggestionLimit();
    const frame = new OverlayFrame(this.theme);
    const lines: string[] = [
      frame.top(contentWidth),
      frame.content(this.theme.fg("accent", this.options.title), contentWidth),
      frame.separator(contentWidth),
      frame.content(`${this.theme.fg("dim", label)}${inputLine}`, contentWidth),
      frame.separator(contentWidth),
    ];

    if (this.error) {
      lines.push(frame.content(this.theme.fg("error", this.error), contentWidth));
    } else if (this.suggestions.length === 0) {
      lines.push(frame.content(this.theme.fg("dim", this.options.emptyMessage ?? "没有匹配的文件"), contentWidth));
    } else {
      for (let i = 0; i < limit; i++) {
        lines.push(frame.content(this.renderSuggestion(this.suggestions[i], i, contentWidth), contentWidth));
      }
    }

    lines.push(frame.separator(contentWidth));
    lines.push(frame.content(this.footerText(contentWidth), contentWidth));
    lines.push(frame.bottom(contentWidth));
    return lines;
  }

  invalidate() {}

  private submit(value: string): void {
    const path = normalizePathInput(value);
    if (!path) {
      this.showError("请选择要导入的文件");
      return;
    }
    try {
      accessSync(path, constants.R_OK);
      if (!statSync(path).isFile()) {
        this.showError("请选择文件，而不是目录");
        return;
      }
    } catch {
      this.showError("文件不存在或无法读取");
      return;
    }
    if (!this.matchesAllowedFile(path)) {
      this.showError(`仅支持 ${this.options.fileExtensions?.join("、")} 文件`);
      return;
    }
    this.done(path);
  }

  private showError(message: string): void {
    this.error = message;
    this.tui.requestRender();
  }

  private moveSelection(delta: number): void {
    if (this.suggestions.length === 0) return;
    this.selectedIndex = Math.max(0, Math.min(this.suggestions.length - 1, this.selectedIndex + delta));
    this.tui.requestRender();
  }

  private completeSelection(): void {
    this.refreshSuggestions();
    const selected = this.selectedSuggestion();
    if (selected) this.acceptSuggestion(selected);
    else this.tui.requestRender();
  }

  private acceptSuggestion(suggestion: PathInputSuggestion): void {
    this.error = undefined;
    this.input.setValue(suggestion.value);
    this.refreshSuggestions();
    this.tui.requestRender();
  }

  private selectedSuggestion(): PathInputSuggestion | undefined {
    return this.suggestions[this.selectedIndex];
  }

  private refreshSuggestions(): void {
    const input = cleanPathInput(this.input.getValue());
    const { dirInput, namePrefix } = splitPathForCompletion(input);
    const fsDir = expandHomePath(dirInput);
    const prefix = namePrefix.toLocaleLowerCase();
    const includeHidden = namePrefix.startsWith(".");

    try {
      this.suggestions = readdirSync(fsDir, { withFileTypes: true })
        .filter((entry) => includeHidden || !entry.name.startsWith("."))
        .filter((entry) => entry.name.toLocaleLowerCase().startsWith(prefix))
        .filter((entry) => entry.isDirectory() || this.matchesAllowedFile(entry.name))
        .sort((a, b) => {
          if (a.isDirectory() !== b.isDirectory()) return a.isDirectory() ? -1 : 1;
          return a.name.localeCompare(b.name);
        })
        .slice(0, this.suggestionLimit())
        .map((entry) => ({
          value: appendPathEntry(dirInput, entry.name, entry.isDirectory()),
          label: entry.name,
          directory: entry.isDirectory(),
        }));
    } catch {
      this.suggestions = [];
    }

    this.selectedIndex = clampIndex(this.selectedIndex, this.suggestions.length);
  }

  private matchesAllowedFile(name: string): boolean {
    const extensions = this.options.fileExtensions ?? [];
    if (extensions.length === 0) return true;
    const lowerName = name.toLocaleLowerCase();
    return extensions.some((extension) => lowerName.endsWith(extension.toLocaleLowerCase()));
  }

  private renderSuggestion(suggestion: PathInputSuggestion | undefined, row: number, width: number): string {
    if (!suggestion) return "";
    const prefix = row === this.selectedIndex ? "› " : "  ";
    const kind = suggestion.directory
      ? `[${this.options.directoryLabel ?? "目录"}]`
      : `[${this.options.fileLabel ?? "文件"}]`;
    const text = truncateToWidth(`${prefix}${kind} ${suggestion.label}`, width, "", true);
    return row === this.selectedIndex ? this.theme.fg("accent", text) : this.theme.fg("dim", text);
  }

  private footerText(width: number): string {
    return this.theme.fg(
      "dim",
      truncateToWidth(this.options.footer ?? "Tab 补全 · ↑↓ 选择 · Enter 确认 · Esc 取消", width),
    );
  }

  private suggestionLimit(): number {
    return this.options.suggestionLimit ?? 6;
  }
}

function clampIndex(index: number, length: number): number {
  if (length <= 0) return 0;
  return Math.max(0, Math.min(length - 1, index));
}

function expandHomePath(path: string): string {
  if (path === "~") return homedir();
  if (path.startsWith("~/") || path.startsWith("~\\")) return `${homedir()}${path.slice(1)}`;
  return path || ".";
}

function splitPathForCompletion(path: string): { dirInput: string; namePrefix: string } {
  if (path === "~") return { dirInput: "~/", namePrefix: "" };
  const lastSeparator = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (lastSeparator === -1) return { dirInput: "", namePrefix: path };
  return {
    dirInput: path.slice(0, lastSeparator + 1),
    namePrefix: path.slice(lastSeparator + 1),
  };
}

function appendPathEntry(dirInput: string, name: string, directory: boolean): string {
  if (!directory) return `${dirInput}${name}`;
  const separator = dirInput.includes("\\") && !dirInput.includes("/") ? "\\" : "/";
  return `${dirInput}${name}${separator}`;
}
