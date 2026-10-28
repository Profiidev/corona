declare module "gpui-kit" {
  export interface El {
    /** Lays the children out with flexbox. */
    flex(): El;
    /** Background color as `0xRRGGBB`. */
    bg(color: number): El;
    /** Space between the children, in pixels. */
    gap(size: number): El;
    /** Fills the parent. */
    size_full(): El;
    /** Appends an element or a text. */
    child(child: El | string): El;
    /** Calls `handler` when the element is clicked. */
    on_click(handler: (event: unknown, cx: ViewCx) => void): El;
  }
  export function div(): El;
  export interface ViewCx {
    /** Renders the view again. */
    notify(): void;
  }
  export abstract class View {
    init?(props: unknown, cx: ViewCx): void;
    abstract render(cx: ViewCx): El;
  }
}
declare module "corona/test" {
  /** Adds `by` to the shared counter. Throws when it would pass 100. */
  export function add(by: number): number;
  /** Resolves with `value` doubled after `ms` milliseconds. */
  export function double_later(ms: number, value: number): Promise<number>;
}
