import { Input, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import type { Component, TUI } from "@earendil-works/pi-tui";
import { OverlayFrame, type OverlayTheme } from "./overlayFrame.ts";

export interface TextInputOverlayOptions {
  title: string;
  label?: string;
  initialValue?: string;
  emptyMessage?: string;
  footer?: string;
  body?: (theme: OverlayTheme, width: number) => string[];
}

/**
 * Adapted from Chenggou1/FishRead's
 * packages/pi-extension/src/components/text-input-overlay.ts for the second
 * step of Fishword's custom-deck import flow.
 */
export class TextInputOverlay implements Component {
  private readonly input = new Input();
  private error: string | undefined;

  constructor(
    private theme: OverlayTheme,
    private tui: TUI,
    private done: (result: string | undefined) => void,
    private options: TextInputOverlayOptions,
  ) {
    this.input.setValue(options.initialValue ?? "");
    this.input.onSubmit = (value) => this.submit(value);
    this.input.onEscape = () => this.done(undefined);
  }

  handleInput(data: string): void {
    this.error = undefined;
    this.input.handleInput(data);
    this.tui.requestRender();
  }

  render(width: number): string[] {
    const contentWidth = Math.max(1, width - 2);
    const label = `${this.options.label ?? "名称"} `;
    const inputWidth = Math.max(1, contentWidth - visibleWidth(label));
    const inputLine = this.input.render(inputWidth)[0] ?? "";
    const body = this.options.body?.(this.theme, contentWidth) ?? [];
    const frame = new OverlayFrame(this.theme);
    const rows = [
      this.theme.fg("accent", this.options.title),
      ...body,
      "",
      `${this.theme.fg("dim", label)}${inputLine}`,
      this.theme.fg(
        this.error ? "error" : "dim",
        truncateToWidth(this.error ?? this.options.footer ?? "Enter 确认 · Esc 返回", contentWidth),
      ),
    ];

    return [
      frame.top(contentWidth),
      ...rows.map((row) => frame.content(row, contentWidth)),
      frame.bottom(contentWidth),
    ];
  }

  invalidate() {}

  private submit(value: string): void {
    const name = value.trim();
    if (!name) {
      this.error = this.options.emptyMessage ?? "词库名称不能为空";
      this.tui.requestRender();
      return;
    }
    this.done(name);
  }
}
