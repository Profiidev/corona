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
/** A value of the shell: call it for the current one. */
interface Signal<T> {
  (): T;
  /** Calls `cb` with every new value; returns the fn that stops it. */
  subscribe(cb: (value: T) => void): () => void;
}
declare module "corona/test" {
  /**  A corner of a shape. */
  export type Point = {
    /**  Pixels from the left. */
    x: number,
    y: number,
  };

  export type Shape = {
    name: string,
    corners: Point[],
    scale: number | null,
  };
  /** Adds `by` to the shared counter. Throws when it would pass 100. */
  export function add(by: number): number;
  /** A square named `name`, `scale` times the counter wide. */
  export function square(name: string, scale?: number | null): Shape;
  /** Resolves with `value` doubled after `ms` milliseconds. */
  export function doubleLater(ms: number, value: number): Promise<number>;
  /** Rejects when `fail`. */
  export function check(fail: boolean): Promise<number>;
  /** Only in another variant of the module. */
  export function hidden(): number;
  /** The watched count. */
  export const count: Signal<number>;
}
