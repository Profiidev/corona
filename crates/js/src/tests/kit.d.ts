declare module "corona/kit" {
  export type SpacingToken = "xxs" | "xs" | "sm" | "md" | "lg" | "xl" | "xxl";
  export type RadiusToken = "none" | "sm" | "md" | "lg" | "xl" | "full";
  export type ColorToken = "background" | "foreground" | "surface" | "surface_foreground" | "primary" | "primary_foreground" | "secondary" | "secondary_foreground" | "muted" | "muted_foreground" | "accent" | "accent_foreground" | "destructive" | "destructive_foreground" | "border" | "input" | "ring" | "selection";
  /** A theme token, `"#rrggbb"`, `"#rrggbbaa"` or `0xrrggbb`. */
  export type Color = ColorToken | `#${string}` | number;
  /** A bare number is pixels; a string carries its unit, or is a spacing token. */
  export type Length = number | `${number}px` | `${number}rem` | `${number}%` | "auto" | SpacingToken;
  export type DefiniteLength = number | `${number}px` | `${number}rem` | `${number}%` | SpacingToken;
  export type AbsoluteLength = number | `${number}px` | `${number}rem`;
  export type Props = Record<string, any>;
  export interface Modifiers {
    shift: boolean;
    control: boolean;
    alt: boolean;
    /** Command on macOS, Super elsewhere. */
    platform: boolean;
    function: boolean;
  }
  export interface Point { x: number; y: number; }
  export interface Size { width: number; height: number; }
  export interface Bounds extends Point, Size {}
  export type MouseButton = "left" | "right" | "middle" | "back" | "forward";
  /** What an `on_click` handler gets. Keyboard activation counts as one. */
  export interface ClickEvent { click_count: number; position: Point; modifiers: Modifiers; }
  /** What `on_mouse_down`, `on_mouse_up` and `on_mouse_down_out` handlers get. */
  export interface MouseButtonEvent {
    button: MouseButton;
    /** `2` on a double click. */
    click_count: number;
    position: Point;
    modifiers: Modifiers;
  }
  /** What an `on_mouse_move` handler gets. `position` is window-relative. */
  export interface MouseMoveEvent { position: Point; pressed_button?: MouseButton; modifiers: Modifiers; }
  /** What an `on_scroll_wheel` handler gets. */
  export interface ScrollWheelEvent {
    /** The distance in pixels, whichever unit the device reported. */
    delta: Point;
    /** The same distance in lines, when the device reported lines. */
    delta_lines?: Point;
    touch_phase: "started" | "moved" | "ended" | "cancelled";
    position: Point;
    modifiers: Modifiers;
  }
  /** What `on_key_down` and `on_key_up` handlers get. */
  export interface KeyEvent {
    /** The key, e.g. `"s"` or `"escape"`. */
    key: string;
    /** The chord as a key binding spells it, `cmd` for the platform key everywhere: `"cmd-shift-s"`. */
    keystroke: string;
    /** The character this keystroke types, if any. */
    key_char?: string;
    modifiers: Modifiers;
    /** Whether the key is held down. Absent on `on_key_up`. */
    is_held?: boolean;
  }
  /** What an `on_modifiers_changed` handler gets. */
  export interface ModifiersChangedEvent { modifiers: Modifiers; capslock: { on: boolean }; }
  /** What an `on_action` handler gets. */
  export interface ActionEvent { action: string; }
  /** Every no-argument style method of gpui's `Styled` and gpui-base's `StyledExt`. */
  export type NullaryStyle =
    | "w_0"
    | "w_neg_0"
    | "w_0p5"
    | "w_neg_0p5"
    | "w_1"
    | "w_neg_1"
    | "w_1p5"
    | "w_neg_1p5"
    | "w_2"
    | "w_neg_2"
    | "w_2p5"
    | "w_neg_2p5"
    | "w_3"
    | "w_neg_3"
    | "w_3p5"
    | "w_neg_3p5"
    | "w_4"
    | "w_neg_4"
    | "w_5"
    | "w_neg_5"
    | "w_6"
    | "w_neg_6"
    | "w_7"
    | "w_neg_7"
    | "w_8"
    | "w_neg_8"
    | "w_9"
    | "w_neg_9"
    | "w_10"
    | "w_neg_10"
    | "w_11"
    | "w_neg_11"
    | "w_12"
    | "w_neg_12"
    | "w_16"
    | "w_neg_16"
    | "w_20"
    | "w_neg_20"
    | "w_24"
    | "w_neg_24"
    | "w_32"
    | "w_neg_32"
    | "w_40"
    | "w_neg_40"
    | "w_48"
    | "w_neg_48"
    | "w_56"
    | "w_neg_56"
    | "w_64"
    | "w_neg_64"
    | "w_72"
    | "w_neg_72"
    | "w_80"
    | "w_neg_80"
    | "w_96"
    | "w_neg_96"
    | "w_112"
    | "w_neg_112"
    | "w_128"
    | "w_neg_128"
    | "w_auto"
    | "w_px"
    | "w_neg_px"
    | "w_full"
    | "w_neg_full"
    | "w_1_2"
    | "w_neg_1_2"
    | "w_1_3"
    | "w_neg_1_3"
    | "w_2_3"
    | "w_neg_2_3"
    | "w_1_4"
    | "w_neg_1_4"
    | "w_2_4"
    | "w_neg_2_4"
    | "w_3_4"
    | "w_neg_3_4"
    | "w_1_5"
    | "w_neg_1_5"
    | "w_2_5"
    | "w_neg_2_5"
    | "w_3_5"
    | "w_neg_3_5"
    | "w_4_5"
    | "w_neg_4_5"
    | "w_1_6"
    | "w_neg_1_6"
    | "w_5_6"
    | "w_neg_5_6"
    | "w_1_12"
    | "w_neg_1_12"
    | "h_0"
    | "h_neg_0"
    | "h_0p5"
    | "h_neg_0p5"
    | "h_1"
    | "h_neg_1"
    | "h_1p5"
    | "h_neg_1p5"
    | "h_2"
    | "h_neg_2"
    | "h_2p5"
    | "h_neg_2p5"
    | "h_3"
    | "h_neg_3"
    | "h_3p5"
    | "h_neg_3p5"
    | "h_4"
    | "h_neg_4"
    | "h_5"
    | "h_neg_5"
    | "h_6"
    | "h_neg_6"
    | "h_7"
    | "h_neg_7"
    | "h_8"
    | "h_neg_8"
    | "h_9"
    | "h_neg_9"
    | "h_10"
    | "h_neg_10"
    | "h_11"
    | "h_neg_11"
    | "h_12"
    | "h_neg_12"
    | "h_16"
    | "h_neg_16"
    | "h_20"
    | "h_neg_20"
    | "h_24"
    | "h_neg_24"
    | "h_32"
    | "h_neg_32"
    | "h_40"
    | "h_neg_40"
    | "h_48"
    | "h_neg_48"
    | "h_56"
    | "h_neg_56"
    | "h_64"
    | "h_neg_64"
    | "h_72"
    | "h_neg_72"
    | "h_80"
    | "h_neg_80"
    | "h_96"
    | "h_neg_96"
    | "h_112"
    | "h_neg_112"
    | "h_128"
    | "h_neg_128"
    | "h_auto"
    | "h_px"
    | "h_neg_px"
    | "h_full"
    | "h_neg_full"
    | "h_1_2"
    | "h_neg_1_2"
    | "h_1_3"
    | "h_neg_1_3"
    | "h_2_3"
    | "h_neg_2_3"
    | "h_1_4"
    | "h_neg_1_4"
    | "h_2_4"
    | "h_neg_2_4"
    | "h_3_4"
    | "h_neg_3_4"
    | "h_1_5"
    | "h_neg_1_5"
    | "h_2_5"
    | "h_neg_2_5"
    | "h_3_5"
    | "h_neg_3_5"
    | "h_4_5"
    | "h_neg_4_5"
    | "h_1_6"
    | "h_neg_1_6"
    | "h_5_6"
    | "h_neg_5_6"
    | "h_1_12"
    | "h_neg_1_12"
    | "size_0"
    | "size_neg_0"
    | "size_0p5"
    | "size_neg_0p5"
    | "size_1"
    | "size_neg_1"
    | "size_1p5"
    | "size_neg_1p5"
    | "size_2"
    | "size_neg_2"
    | "size_2p5"
    | "size_neg_2p5"
    | "size_3"
    | "size_neg_3"
    | "size_3p5"
    | "size_neg_3p5"
    | "size_4"
    | "size_neg_4"
    | "size_5"
    | "size_neg_5"
    | "size_6"
    | "size_neg_6"
    | "size_7"
    | "size_neg_7"
    | "size_8"
    | "size_neg_8"
    | "size_9"
    | "size_neg_9"
    | "size_10"
    | "size_neg_10"
    | "size_11"
    | "size_neg_11"
    | "size_12"
    | "size_neg_12"
    | "size_16"
    | "size_neg_16"
    | "size_20"
    | "size_neg_20"
    | "size_24"
    | "size_neg_24"
    | "size_32"
    | "size_neg_32"
    | "size_40"
    | "size_neg_40"
    | "size_48"
    | "size_neg_48"
    | "size_56"
    | "size_neg_56"
    | "size_64"
    | "size_neg_64"
    | "size_72"
    | "size_neg_72"
    | "size_80"
    | "size_neg_80"
    | "size_96"
    | "size_neg_96"
    | "size_112"
    | "size_neg_112"
    | "size_128"
    | "size_neg_128"
    | "size_auto"
    | "size_px"
    | "size_neg_px"
    | "size_full"
    | "size_neg_full"
    | "size_1_2"
    | "size_neg_1_2"
    | "size_1_3"
    | "size_neg_1_3"
    | "size_2_3"
    | "size_neg_2_3"
    | "size_1_4"
    | "size_neg_1_4"
    | "size_2_4"
    | "size_neg_2_4"
    | "size_3_4"
    | "size_neg_3_4"
    | "size_1_5"
    | "size_neg_1_5"
    | "size_2_5"
    | "size_neg_2_5"
    | "size_3_5"
    | "size_neg_3_5"
    | "size_4_5"
    | "size_neg_4_5"
    | "size_1_6"
    | "size_neg_1_6"
    | "size_5_6"
    | "size_neg_5_6"
    | "size_1_12"
    | "size_neg_1_12"
    | "min_size_0"
    | "min_size_neg_0"
    | "min_size_0p5"
    | "min_size_neg_0p5"
    | "min_size_1"
    | "min_size_neg_1"
    | "min_size_1p5"
    | "min_size_neg_1p5"
    | "min_size_2"
    | "min_size_neg_2"
    | "min_size_2p5"
    | "min_size_neg_2p5"
    | "min_size_3"
    | "min_size_neg_3"
    | "min_size_3p5"
    | "min_size_neg_3p5"
    | "min_size_4"
    | "min_size_neg_4"
    | "min_size_5"
    | "min_size_neg_5"
    | "min_size_6"
    | "min_size_neg_6"
    | "min_size_7"
    | "min_size_neg_7"
    | "min_size_8"
    | "min_size_neg_8"
    | "min_size_9"
    | "min_size_neg_9"
    | "min_size_10"
    | "min_size_neg_10"
    | "min_size_11"
    | "min_size_neg_11"
    | "min_size_12"
    | "min_size_neg_12"
    | "min_size_16"
    | "min_size_neg_16"
    | "min_size_20"
    | "min_size_neg_20"
    | "min_size_24"
    | "min_size_neg_24"
    | "min_size_32"
    | "min_size_neg_32"
    | "min_size_40"
    | "min_size_neg_40"
    | "min_size_48"
    | "min_size_neg_48"
    | "min_size_56"
    | "min_size_neg_56"
    | "min_size_64"
    | "min_size_neg_64"
    | "min_size_72"
    | "min_size_neg_72"
    | "min_size_80"
    | "min_size_neg_80"
    | "min_size_96"
    | "min_size_neg_96"
    | "min_size_112"
    | "min_size_neg_112"
    | "min_size_128"
    | "min_size_neg_128"
    | "min_size_auto"
    | "min_size_px"
    | "min_size_neg_px"
    | "min_size_full"
    | "min_size_neg_full"
    | "min_size_1_2"
    | "min_size_neg_1_2"
    | "min_size_1_3"
    | "min_size_neg_1_3"
    | "min_size_2_3"
    | "min_size_neg_2_3"
    | "min_size_1_4"
    | "min_size_neg_1_4"
    | "min_size_2_4"
    | "min_size_neg_2_4"
    | "min_size_3_4"
    | "min_size_neg_3_4"
    | "min_size_1_5"
    | "min_size_neg_1_5"
    | "min_size_2_5"
    | "min_size_neg_2_5"
    | "min_size_3_5"
    | "min_size_neg_3_5"
    | "min_size_4_5"
    | "min_size_neg_4_5"
    | "min_size_1_6"
    | "min_size_neg_1_6"
    | "min_size_5_6"
    | "min_size_neg_5_6"
    | "min_size_1_12"
    | "min_size_neg_1_12"
    | "min_w_0"
    | "min_w_neg_0"
    | "min_w_0p5"
    | "min_w_neg_0p5"
    | "min_w_1"
    | "min_w_neg_1"
    | "min_w_1p5"
    | "min_w_neg_1p5"
    | "min_w_2"
    | "min_w_neg_2"
    | "min_w_2p5"
    | "min_w_neg_2p5"
    | "min_w_3"
    | "min_w_neg_3"
    | "min_w_3p5"
    | "min_w_neg_3p5"
    | "min_w_4"
    | "min_w_neg_4"
    | "min_w_5"
    | "min_w_neg_5"
    | "min_w_6"
    | "min_w_neg_6"
    | "min_w_7"
    | "min_w_neg_7"
    | "min_w_8"
    | "min_w_neg_8"
    | "min_w_9"
    | "min_w_neg_9"
    | "min_w_10"
    | "min_w_neg_10"
    | "min_w_11"
    | "min_w_neg_11"
    | "min_w_12"
    | "min_w_neg_12"
    | "min_w_16"
    | "min_w_neg_16"
    | "min_w_20"
    | "min_w_neg_20"
    | "min_w_24"
    | "min_w_neg_24"
    | "min_w_32"
    | "min_w_neg_32"
    | "min_w_40"
    | "min_w_neg_40"
    | "min_w_48"
    | "min_w_neg_48"
    | "min_w_56"
    | "min_w_neg_56"
    | "min_w_64"
    | "min_w_neg_64"
    | "min_w_72"
    | "min_w_neg_72"
    | "min_w_80"
    | "min_w_neg_80"
    | "min_w_96"
    | "min_w_neg_96"
    | "min_w_112"
    | "min_w_neg_112"
    | "min_w_128"
    | "min_w_neg_128"
    | "min_w_auto"
    | "min_w_px"
    | "min_w_neg_px"
    | "min_w_full"
    | "min_w_neg_full"
    | "min_w_1_2"
    | "min_w_neg_1_2"
    | "min_w_1_3"
    | "min_w_neg_1_3"
    | "min_w_2_3"
    | "min_w_neg_2_3"
    | "min_w_1_4"
    | "min_w_neg_1_4"
    | "min_w_2_4"
    | "min_w_neg_2_4"
    | "min_w_3_4"
    | "min_w_neg_3_4"
    | "min_w_1_5"
    | "min_w_neg_1_5"
    | "min_w_2_5"
    | "min_w_neg_2_5"
    | "min_w_3_5"
    | "min_w_neg_3_5"
    | "min_w_4_5"
    | "min_w_neg_4_5"
    | "min_w_1_6"
    | "min_w_neg_1_6"
    | "min_w_5_6"
    | "min_w_neg_5_6"
    | "min_w_1_12"
    | "min_w_neg_1_12"
    | "min_h_0"
    | "min_h_neg_0"
    | "min_h_0p5"
    | "min_h_neg_0p5"
    | "min_h_1"
    | "min_h_neg_1"
    | "min_h_1p5"
    | "min_h_neg_1p5"
    | "min_h_2"
    | "min_h_neg_2"
    | "min_h_2p5"
    | "min_h_neg_2p5"
    | "min_h_3"
    | "min_h_neg_3"
    | "min_h_3p5"
    | "min_h_neg_3p5"
    | "min_h_4"
    | "min_h_neg_4"
    | "min_h_5"
    | "min_h_neg_5"
    | "min_h_6"
    | "min_h_neg_6"
    | "min_h_7"
    | "min_h_neg_7"
    | "min_h_8"
    | "min_h_neg_8"
    | "min_h_9"
    | "min_h_neg_9"
    | "min_h_10"
    | "min_h_neg_10"
    | "min_h_11"
    | "min_h_neg_11"
    | "min_h_12"
    | "min_h_neg_12"
    | "min_h_16"
    | "min_h_neg_16"
    | "min_h_20"
    | "min_h_neg_20"
    | "min_h_24"
    | "min_h_neg_24"
    | "min_h_32"
    | "min_h_neg_32"
    | "min_h_40"
    | "min_h_neg_40"
    | "min_h_48"
    | "min_h_neg_48"
    | "min_h_56"
    | "min_h_neg_56"
    | "min_h_64"
    | "min_h_neg_64"
    | "min_h_72"
    | "min_h_neg_72"
    | "min_h_80"
    | "min_h_neg_80"
    | "min_h_96"
    | "min_h_neg_96"
    | "min_h_112"
    | "min_h_neg_112"
    | "min_h_128"
    | "min_h_neg_128"
    | "min_h_auto"
    | "min_h_px"
    | "min_h_neg_px"
    | "min_h_full"
    | "min_h_neg_full"
    | "min_h_1_2"
    | "min_h_neg_1_2"
    | "min_h_1_3"
    | "min_h_neg_1_3"
    | "min_h_2_3"
    | "min_h_neg_2_3"
    | "min_h_1_4"
    | "min_h_neg_1_4"
    | "min_h_2_4"
    | "min_h_neg_2_4"
    | "min_h_3_4"
    | "min_h_neg_3_4"
    | "min_h_1_5"
    | "min_h_neg_1_5"
    | "min_h_2_5"
    | "min_h_neg_2_5"
    | "min_h_3_5"
    | "min_h_neg_3_5"
    | "min_h_4_5"
    | "min_h_neg_4_5"
    | "min_h_1_6"
    | "min_h_neg_1_6"
    | "min_h_5_6"
    | "min_h_neg_5_6"
    | "min_h_1_12"
    | "min_h_neg_1_12"
    | "max_size_0"
    | "max_size_neg_0"
    | "max_size_0p5"
    | "max_size_neg_0p5"
    | "max_size_1"
    | "max_size_neg_1"
    | "max_size_1p5"
    | "max_size_neg_1p5"
    | "max_size_2"
    | "max_size_neg_2"
    | "max_size_2p5"
    | "max_size_neg_2p5"
    | "max_size_3"
    | "max_size_neg_3"
    | "max_size_3p5"
    | "max_size_neg_3p5"
    | "max_size_4"
    | "max_size_neg_4"
    | "max_size_5"
    | "max_size_neg_5"
    | "max_size_6"
    | "max_size_neg_6"
    | "max_size_7"
    | "max_size_neg_7"
    | "max_size_8"
    | "max_size_neg_8"
    | "max_size_9"
    | "max_size_neg_9"
    | "max_size_10"
    | "max_size_neg_10"
    | "max_size_11"
    | "max_size_neg_11"
    | "max_size_12"
    | "max_size_neg_12"
    | "max_size_16"
    | "max_size_neg_16"
    | "max_size_20"
    | "max_size_neg_20"
    | "max_size_24"
    | "max_size_neg_24"
    | "max_size_32"
    | "max_size_neg_32"
    | "max_size_40"
    | "max_size_neg_40"
    | "max_size_48"
    | "max_size_neg_48"
    | "max_size_56"
    | "max_size_neg_56"
    | "max_size_64"
    | "max_size_neg_64"
    | "max_size_72"
    | "max_size_neg_72"
    | "max_size_80"
    | "max_size_neg_80"
    | "max_size_96"
    | "max_size_neg_96"
    | "max_size_112"
    | "max_size_neg_112"
    | "max_size_128"
    | "max_size_neg_128"
    | "max_size_auto"
    | "max_size_px"
    | "max_size_neg_px"
    | "max_size_full"
    | "max_size_neg_full"
    | "max_size_1_2"
    | "max_size_neg_1_2"
    | "max_size_1_3"
    | "max_size_neg_1_3"
    | "max_size_2_3"
    | "max_size_neg_2_3"
    | "max_size_1_4"
    | "max_size_neg_1_4"
    | "max_size_2_4"
    | "max_size_neg_2_4"
    | "max_size_3_4"
    | "max_size_neg_3_4"
    | "max_size_1_5"
    | "max_size_neg_1_5"
    | "max_size_2_5"
    | "max_size_neg_2_5"
    | "max_size_3_5"
    | "max_size_neg_3_5"
    | "max_size_4_5"
    | "max_size_neg_4_5"
    | "max_size_1_6"
    | "max_size_neg_1_6"
    | "max_size_5_6"
    | "max_size_neg_5_6"
    | "max_size_1_12"
    | "max_size_neg_1_12"
    | "max_w_0"
    | "max_w_neg_0"
    | "max_w_0p5"
    | "max_w_neg_0p5"
    | "max_w_1"
    | "max_w_neg_1"
    | "max_w_1p5"
    | "max_w_neg_1p5"
    | "max_w_2"
    | "max_w_neg_2"
    | "max_w_2p5"
    | "max_w_neg_2p5"
    | "max_w_3"
    | "max_w_neg_3"
    | "max_w_3p5"
    | "max_w_neg_3p5"
    | "max_w_4"
    | "max_w_neg_4"
    | "max_w_5"
    | "max_w_neg_5"
    | "max_w_6"
    | "max_w_neg_6"
    | "max_w_7"
    | "max_w_neg_7"
    | "max_w_8"
    | "max_w_neg_8"
    | "max_w_9"
    | "max_w_neg_9"
    | "max_w_10"
    | "max_w_neg_10"
    | "max_w_11"
    | "max_w_neg_11"
    | "max_w_12"
    | "max_w_neg_12"
    | "max_w_16"
    | "max_w_neg_16"
    | "max_w_20"
    | "max_w_neg_20"
    | "max_w_24"
    | "max_w_neg_24"
    | "max_w_32"
    | "max_w_neg_32"
    | "max_w_40"
    | "max_w_neg_40"
    | "max_w_48"
    | "max_w_neg_48"
    | "max_w_56"
    | "max_w_neg_56"
    | "max_w_64"
    | "max_w_neg_64"
    | "max_w_72"
    | "max_w_neg_72"
    | "max_w_80"
    | "max_w_neg_80"
    | "max_w_96"
    | "max_w_neg_96"
    | "max_w_112"
    | "max_w_neg_112"
    | "max_w_128"
    | "max_w_neg_128"
    | "max_w_auto"
    | "max_w_px"
    | "max_w_neg_px"
    | "max_w_full"
    | "max_w_neg_full"
    | "max_w_1_2"
    | "max_w_neg_1_2"
    | "max_w_1_3"
    | "max_w_neg_1_3"
    | "max_w_2_3"
    | "max_w_neg_2_3"
    | "max_w_1_4"
    | "max_w_neg_1_4"
    | "max_w_2_4"
    | "max_w_neg_2_4"
    | "max_w_3_4"
    | "max_w_neg_3_4"
    | "max_w_1_5"
    | "max_w_neg_1_5"
    | "max_w_2_5"
    | "max_w_neg_2_5"
    | "max_w_3_5"
    | "max_w_neg_3_5"
    | "max_w_4_5"
    | "max_w_neg_4_5"
    | "max_w_1_6"
    | "max_w_neg_1_6"
    | "max_w_5_6"
    | "max_w_neg_5_6"
    | "max_w_1_12"
    | "max_w_neg_1_12"
    | "max_h_0"
    | "max_h_neg_0"
    | "max_h_0p5"
    | "max_h_neg_0p5"
    | "max_h_1"
    | "max_h_neg_1"
    | "max_h_1p5"
    | "max_h_neg_1p5"
    | "max_h_2"
    | "max_h_neg_2"
    | "max_h_2p5"
    | "max_h_neg_2p5"
    | "max_h_3"
    | "max_h_neg_3"
    | "max_h_3p5"
    | "max_h_neg_3p5"
    | "max_h_4"
    | "max_h_neg_4"
    | "max_h_5"
    | "max_h_neg_5"
    | "max_h_6"
    | "max_h_neg_6"
    | "max_h_7"
    | "max_h_neg_7"
    | "max_h_8"
    | "max_h_neg_8"
    | "max_h_9"
    | "max_h_neg_9"
    | "max_h_10"
    | "max_h_neg_10"
    | "max_h_11"
    | "max_h_neg_11"
    | "max_h_12"
    | "max_h_neg_12"
    | "max_h_16"
    | "max_h_neg_16"
    | "max_h_20"
    | "max_h_neg_20"
    | "max_h_24"
    | "max_h_neg_24"
    | "max_h_32"
    | "max_h_neg_32"
    | "max_h_40"
    | "max_h_neg_40"
    | "max_h_48"
    | "max_h_neg_48"
    | "max_h_56"
    | "max_h_neg_56"
    | "max_h_64"
    | "max_h_neg_64"
    | "max_h_72"
    | "max_h_neg_72"
    | "max_h_80"
    | "max_h_neg_80"
    | "max_h_96"
    | "max_h_neg_96"
    | "max_h_112"
    | "max_h_neg_112"
    | "max_h_128"
    | "max_h_neg_128"
    | "max_h_auto"
    | "max_h_px"
    | "max_h_neg_px"
    | "max_h_full"
    | "max_h_neg_full"
    | "max_h_1_2"
    | "max_h_neg_1_2"
    | "max_h_1_3"
    | "max_h_neg_1_3"
    | "max_h_2_3"
    | "max_h_neg_2_3"
    | "max_h_1_4"
    | "max_h_neg_1_4"
    | "max_h_2_4"
    | "max_h_neg_2_4"
    | "max_h_3_4"
    | "max_h_neg_3_4"
    | "max_h_1_5"
    | "max_h_neg_1_5"
    | "max_h_2_5"
    | "max_h_neg_2_5"
    | "max_h_3_5"
    | "max_h_neg_3_5"
    | "max_h_4_5"
    | "max_h_neg_4_5"
    | "max_h_1_6"
    | "max_h_neg_1_6"
    | "max_h_5_6"
    | "max_h_neg_5_6"
    | "max_h_1_12"
    | "max_h_neg_1_12"
    | "gap_0"
    | "gap_neg_0"
    | "gap_0p5"
    | "gap_neg_0p5"
    | "gap_1"
    | "gap_neg_1"
    | "gap_1p5"
    | "gap_neg_1p5"
    | "gap_2"
    | "gap_neg_2"
    | "gap_2p5"
    | "gap_neg_2p5"
    | "gap_3"
    | "gap_neg_3"
    | "gap_3p5"
    | "gap_neg_3p5"
    | "gap_4"
    | "gap_neg_4"
    | "gap_5"
    | "gap_neg_5"
    | "gap_6"
    | "gap_neg_6"
    | "gap_7"
    | "gap_neg_7"
    | "gap_8"
    | "gap_neg_8"
    | "gap_9"
    | "gap_neg_9"
    | "gap_10"
    | "gap_neg_10"
    | "gap_11"
    | "gap_neg_11"
    | "gap_12"
    | "gap_neg_12"
    | "gap_16"
    | "gap_neg_16"
    | "gap_20"
    | "gap_neg_20"
    | "gap_24"
    | "gap_neg_24"
    | "gap_32"
    | "gap_neg_32"
    | "gap_40"
    | "gap_neg_40"
    | "gap_48"
    | "gap_neg_48"
    | "gap_56"
    | "gap_neg_56"
    | "gap_64"
    | "gap_neg_64"
    | "gap_72"
    | "gap_neg_72"
    | "gap_80"
    | "gap_neg_80"
    | "gap_96"
    | "gap_neg_96"
    | "gap_112"
    | "gap_neg_112"
    | "gap_128"
    | "gap_neg_128"
    | "gap_px"
    | "gap_neg_px"
    | "gap_full"
    | "gap_neg_full"
    | "gap_1_2"
    | "gap_neg_1_2"
    | "gap_1_3"
    | "gap_neg_1_3"
    | "gap_2_3"
    | "gap_neg_2_3"
    | "gap_1_4"
    | "gap_neg_1_4"
    | "gap_2_4"
    | "gap_neg_2_4"
    | "gap_3_4"
    | "gap_neg_3_4"
    | "gap_1_5"
    | "gap_neg_1_5"
    | "gap_2_5"
    | "gap_neg_2_5"
    | "gap_3_5"
    | "gap_neg_3_5"
    | "gap_4_5"
    | "gap_neg_4_5"
    | "gap_1_6"
    | "gap_neg_1_6"
    | "gap_5_6"
    | "gap_neg_5_6"
    | "gap_1_12"
    | "gap_neg_1_12"
    | "gap_x_0"
    | "gap_x_neg_0"
    | "gap_x_0p5"
    | "gap_x_neg_0p5"
    | "gap_x_1"
    | "gap_x_neg_1"
    | "gap_x_1p5"
    | "gap_x_neg_1p5"
    | "gap_x_2"
    | "gap_x_neg_2"
    | "gap_x_2p5"
    | "gap_x_neg_2p5"
    | "gap_x_3"
    | "gap_x_neg_3"
    | "gap_x_3p5"
    | "gap_x_neg_3p5"
    | "gap_x_4"
    | "gap_x_neg_4"
    | "gap_x_5"
    | "gap_x_neg_5"
    | "gap_x_6"
    | "gap_x_neg_6"
    | "gap_x_7"
    | "gap_x_neg_7"
    | "gap_x_8"
    | "gap_x_neg_8"
    | "gap_x_9"
    | "gap_x_neg_9"
    | "gap_x_10"
    | "gap_x_neg_10"
    | "gap_x_11"
    | "gap_x_neg_11"
    | "gap_x_12"
    | "gap_x_neg_12"
    | "gap_x_16"
    | "gap_x_neg_16"
    | "gap_x_20"
    | "gap_x_neg_20"
    | "gap_x_24"
    | "gap_x_neg_24"
    | "gap_x_32"
    | "gap_x_neg_32"
    | "gap_x_40"
    | "gap_x_neg_40"
    | "gap_x_48"
    | "gap_x_neg_48"
    | "gap_x_56"
    | "gap_x_neg_56"
    | "gap_x_64"
    | "gap_x_neg_64"
    | "gap_x_72"
    | "gap_x_neg_72"
    | "gap_x_80"
    | "gap_x_neg_80"
    | "gap_x_96"
    | "gap_x_neg_96"
    | "gap_x_112"
    | "gap_x_neg_112"
    | "gap_x_128"
    | "gap_x_neg_128"
    | "gap_x_px"
    | "gap_x_neg_px"
    | "gap_x_full"
    | "gap_x_neg_full"
    | "gap_x_1_2"
    | "gap_x_neg_1_2"
    | "gap_x_1_3"
    | "gap_x_neg_1_3"
    | "gap_x_2_3"
    | "gap_x_neg_2_3"
    | "gap_x_1_4"
    | "gap_x_neg_1_4"
    | "gap_x_2_4"
    | "gap_x_neg_2_4"
    | "gap_x_3_4"
    | "gap_x_neg_3_4"
    | "gap_x_1_5"
    | "gap_x_neg_1_5"
    | "gap_x_2_5"
    | "gap_x_neg_2_5"
    | "gap_x_3_5"
    | "gap_x_neg_3_5"
    | "gap_x_4_5"
    | "gap_x_neg_4_5"
    | "gap_x_1_6"
    | "gap_x_neg_1_6"
    | "gap_x_5_6"
    | "gap_x_neg_5_6"
    | "gap_x_1_12"
    | "gap_x_neg_1_12"
    | "gap_y_0"
    | "gap_y_neg_0"
    | "gap_y_0p5"
    | "gap_y_neg_0p5"
    | "gap_y_1"
    | "gap_y_neg_1"
    | "gap_y_1p5"
    | "gap_y_neg_1p5"
    | "gap_y_2"
    | "gap_y_neg_2"
    | "gap_y_2p5"
    | "gap_y_neg_2p5"
    | "gap_y_3"
    | "gap_y_neg_3"
    | "gap_y_3p5"
    | "gap_y_neg_3p5"
    | "gap_y_4"
    | "gap_y_neg_4"
    | "gap_y_5"
    | "gap_y_neg_5"
    | "gap_y_6"
    | "gap_y_neg_6"
    | "gap_y_7"
    | "gap_y_neg_7"
    | "gap_y_8"
    | "gap_y_neg_8"
    | "gap_y_9"
    | "gap_y_neg_9"
    | "gap_y_10"
    | "gap_y_neg_10"
    | "gap_y_11"
    | "gap_y_neg_11"
    | "gap_y_12"
    | "gap_y_neg_12"
    | "gap_y_16"
    | "gap_y_neg_16"
    | "gap_y_20"
    | "gap_y_neg_20"
    | "gap_y_24"
    | "gap_y_neg_24"
    | "gap_y_32"
    | "gap_y_neg_32"
    | "gap_y_40"
    | "gap_y_neg_40"
    | "gap_y_48"
    | "gap_y_neg_48"
    | "gap_y_56"
    | "gap_y_neg_56"
    | "gap_y_64"
    | "gap_y_neg_64"
    | "gap_y_72"
    | "gap_y_neg_72"
    | "gap_y_80"
    | "gap_y_neg_80"
    | "gap_y_96"
    | "gap_y_neg_96"
    | "gap_y_112"
    | "gap_y_neg_112"
    | "gap_y_128"
    | "gap_y_neg_128"
    | "gap_y_px"
    | "gap_y_neg_px"
    | "gap_y_full"
    | "gap_y_neg_full"
    | "gap_y_1_2"
    | "gap_y_neg_1_2"
    | "gap_y_1_3"
    | "gap_y_neg_1_3"
    | "gap_y_2_3"
    | "gap_y_neg_2_3"
    | "gap_y_1_4"
    | "gap_y_neg_1_4"
    | "gap_y_2_4"
    | "gap_y_neg_2_4"
    | "gap_y_3_4"
    | "gap_y_neg_3_4"
    | "gap_y_1_5"
    | "gap_y_neg_1_5"
    | "gap_y_2_5"
    | "gap_y_neg_2_5"
    | "gap_y_3_5"
    | "gap_y_neg_3_5"
    | "gap_y_4_5"
    | "gap_y_neg_4_5"
    | "gap_y_1_6"
    | "gap_y_neg_1_6"
    | "gap_y_5_6"
    | "gap_y_neg_5_6"
    | "gap_y_1_12"
    | "gap_y_neg_1_12"
    | "rounded_none"
    | "rounded_xs"
    | "rounded_sm"
    | "rounded_md"
    | "rounded_lg"
    | "rounded_xl"
    | "rounded_2xl"
    | "rounded_3xl"
    | "rounded_full"
    | "rounded_t_none"
    | "rounded_t_xs"
    | "rounded_t_sm"
    | "rounded_t_md"
    | "rounded_t_lg"
    | "rounded_t_xl"
    | "rounded_t_2xl"
    | "rounded_t_3xl"
    | "rounded_t_full"
    | "rounded_b_none"
    | "rounded_b_xs"
    | "rounded_b_sm"
    | "rounded_b_md"
    | "rounded_b_lg"
    | "rounded_b_xl"
    | "rounded_b_2xl"
    | "rounded_b_3xl"
    | "rounded_b_full"
    | "rounded_r_none"
    | "rounded_r_xs"
    | "rounded_r_sm"
    | "rounded_r_md"
    | "rounded_r_lg"
    | "rounded_r_xl"
    | "rounded_r_2xl"
    | "rounded_r_3xl"
    | "rounded_r_full"
    | "rounded_l_none"
    | "rounded_l_xs"
    | "rounded_l_sm"
    | "rounded_l_md"
    | "rounded_l_lg"
    | "rounded_l_xl"
    | "rounded_l_2xl"
    | "rounded_l_3xl"
    | "rounded_l_full"
    | "rounded_tl_none"
    | "rounded_tl_xs"
    | "rounded_tl_sm"
    | "rounded_tl_md"
    | "rounded_tl_lg"
    | "rounded_tl_xl"
    | "rounded_tl_2xl"
    | "rounded_tl_3xl"
    | "rounded_tl_full"
    | "rounded_tr_none"
    | "rounded_tr_xs"
    | "rounded_tr_sm"
    | "rounded_tr_md"
    | "rounded_tr_lg"
    | "rounded_tr_xl"
    | "rounded_tr_2xl"
    | "rounded_tr_3xl"
    | "rounded_tr_full"
    | "rounded_bl_none"
    | "rounded_bl_xs"
    | "rounded_bl_sm"
    | "rounded_bl_md"
    | "rounded_bl_lg"
    | "rounded_bl_xl"
    | "rounded_bl_2xl"
    | "rounded_bl_3xl"
    | "rounded_bl_full"
    | "rounded_br_none"
    | "rounded_br_xs"
    | "rounded_br_sm"
    | "rounded_br_md"
    | "rounded_br_lg"
    | "rounded_br_xl"
    | "rounded_br_2xl"
    | "rounded_br_3xl"
    | "rounded_br_full"
    | "visible"
    | "invisible"
    | "m_0"
    | "m_neg_0"
    | "m_0p5"
    | "m_neg_0p5"
    | "m_1"
    | "m_neg_1"
    | "m_1p5"
    | "m_neg_1p5"
    | "m_2"
    | "m_neg_2"
    | "m_2p5"
    | "m_neg_2p5"
    | "m_3"
    | "m_neg_3"
    | "m_3p5"
    | "m_neg_3p5"
    | "m_4"
    | "m_neg_4"
    | "m_5"
    | "m_neg_5"
    | "m_6"
    | "m_neg_6"
    | "m_7"
    | "m_neg_7"
    | "m_8"
    | "m_neg_8"
    | "m_9"
    | "m_neg_9"
    | "m_10"
    | "m_neg_10"
    | "m_11"
    | "m_neg_11"
    | "m_12"
    | "m_neg_12"
    | "m_16"
    | "m_neg_16"
    | "m_20"
    | "m_neg_20"
    | "m_24"
    | "m_neg_24"
    | "m_32"
    | "m_neg_32"
    | "m_40"
    | "m_neg_40"
    | "m_48"
    | "m_neg_48"
    | "m_56"
    | "m_neg_56"
    | "m_64"
    | "m_neg_64"
    | "m_72"
    | "m_neg_72"
    | "m_80"
    | "m_neg_80"
    | "m_96"
    | "m_neg_96"
    | "m_112"
    | "m_neg_112"
    | "m_128"
    | "m_neg_128"
    | "m_auto"
    | "m_px"
    | "m_neg_px"
    | "m_full"
    | "m_neg_full"
    | "m_1_2"
    | "m_neg_1_2"
    | "m_1_3"
    | "m_neg_1_3"
    | "m_2_3"
    | "m_neg_2_3"
    | "m_1_4"
    | "m_neg_1_4"
    | "m_2_4"
    | "m_neg_2_4"
    | "m_3_4"
    | "m_neg_3_4"
    | "m_1_5"
    | "m_neg_1_5"
    | "m_2_5"
    | "m_neg_2_5"
    | "m_3_5"
    | "m_neg_3_5"
    | "m_4_5"
    | "m_neg_4_5"
    | "m_1_6"
    | "m_neg_1_6"
    | "m_5_6"
    | "m_neg_5_6"
    | "m_1_12"
    | "m_neg_1_12"
    | "mt_0"
    | "mt_neg_0"
    | "mt_0p5"
    | "mt_neg_0p5"
    | "mt_1"
    | "mt_neg_1"
    | "mt_1p5"
    | "mt_neg_1p5"
    | "mt_2"
    | "mt_neg_2"
    | "mt_2p5"
    | "mt_neg_2p5"
    | "mt_3"
    | "mt_neg_3"
    | "mt_3p5"
    | "mt_neg_3p5"
    | "mt_4"
    | "mt_neg_4"
    | "mt_5"
    | "mt_neg_5"
    | "mt_6"
    | "mt_neg_6"
    | "mt_7"
    | "mt_neg_7"
    | "mt_8"
    | "mt_neg_8"
    | "mt_9"
    | "mt_neg_9"
    | "mt_10"
    | "mt_neg_10"
    | "mt_11"
    | "mt_neg_11"
    | "mt_12"
    | "mt_neg_12"
    | "mt_16"
    | "mt_neg_16"
    | "mt_20"
    | "mt_neg_20"
    | "mt_24"
    | "mt_neg_24"
    | "mt_32"
    | "mt_neg_32"
    | "mt_40"
    | "mt_neg_40"
    | "mt_48"
    | "mt_neg_48"
    | "mt_56"
    | "mt_neg_56"
    | "mt_64"
    | "mt_neg_64"
    | "mt_72"
    | "mt_neg_72"
    | "mt_80"
    | "mt_neg_80"
    | "mt_96"
    | "mt_neg_96"
    | "mt_112"
    | "mt_neg_112"
    | "mt_128"
    | "mt_neg_128"
    | "mt_auto"
    | "mt_px"
    | "mt_neg_px"
    | "mt_full"
    | "mt_neg_full"
    | "mt_1_2"
    | "mt_neg_1_2"
    | "mt_1_3"
    | "mt_neg_1_3"
    | "mt_2_3"
    | "mt_neg_2_3"
    | "mt_1_4"
    | "mt_neg_1_4"
    | "mt_2_4"
    | "mt_neg_2_4"
    | "mt_3_4"
    | "mt_neg_3_4"
    | "mt_1_5"
    | "mt_neg_1_5"
    | "mt_2_5"
    | "mt_neg_2_5"
    | "mt_3_5"
    | "mt_neg_3_5"
    | "mt_4_5"
    | "mt_neg_4_5"
    | "mt_1_6"
    | "mt_neg_1_6"
    | "mt_5_6"
    | "mt_neg_5_6"
    | "mt_1_12"
    | "mt_neg_1_12"
    | "mb_0"
    | "mb_neg_0"
    | "mb_0p5"
    | "mb_neg_0p5"
    | "mb_1"
    | "mb_neg_1"
    | "mb_1p5"
    | "mb_neg_1p5"
    | "mb_2"
    | "mb_neg_2"
    | "mb_2p5"
    | "mb_neg_2p5"
    | "mb_3"
    | "mb_neg_3"
    | "mb_3p5"
    | "mb_neg_3p5"
    | "mb_4"
    | "mb_neg_4"
    | "mb_5"
    | "mb_neg_5"
    | "mb_6"
    | "mb_neg_6"
    | "mb_7"
    | "mb_neg_7"
    | "mb_8"
    | "mb_neg_8"
    | "mb_9"
    | "mb_neg_9"
    | "mb_10"
    | "mb_neg_10"
    | "mb_11"
    | "mb_neg_11"
    | "mb_12"
    | "mb_neg_12"
    | "mb_16"
    | "mb_neg_16"
    | "mb_20"
    | "mb_neg_20"
    | "mb_24"
    | "mb_neg_24"
    | "mb_32"
    | "mb_neg_32"
    | "mb_40"
    | "mb_neg_40"
    | "mb_48"
    | "mb_neg_48"
    | "mb_56"
    | "mb_neg_56"
    | "mb_64"
    | "mb_neg_64"
    | "mb_72"
    | "mb_neg_72"
    | "mb_80"
    | "mb_neg_80"
    | "mb_96"
    | "mb_neg_96"
    | "mb_112"
    | "mb_neg_112"
    | "mb_128"
    | "mb_neg_128"
    | "mb_auto"
    | "mb_px"
    | "mb_neg_px"
    | "mb_full"
    | "mb_neg_full"
    | "mb_1_2"
    | "mb_neg_1_2"
    | "mb_1_3"
    | "mb_neg_1_3"
    | "mb_2_3"
    | "mb_neg_2_3"
    | "mb_1_4"
    | "mb_neg_1_4"
    | "mb_2_4"
    | "mb_neg_2_4"
    | "mb_3_4"
    | "mb_neg_3_4"
    | "mb_1_5"
    | "mb_neg_1_5"
    | "mb_2_5"
    | "mb_neg_2_5"
    | "mb_3_5"
    | "mb_neg_3_5"
    | "mb_4_5"
    | "mb_neg_4_5"
    | "mb_1_6"
    | "mb_neg_1_6"
    | "mb_5_6"
    | "mb_neg_5_6"
    | "mb_1_12"
    | "mb_neg_1_12"
    | "my_0"
    | "my_neg_0"
    | "my_0p5"
    | "my_neg_0p5"
    | "my_1"
    | "my_neg_1"
    | "my_1p5"
    | "my_neg_1p5"
    | "my_2"
    | "my_neg_2"
    | "my_2p5"
    | "my_neg_2p5"
    | "my_3"
    | "my_neg_3"
    | "my_3p5"
    | "my_neg_3p5"
    | "my_4"
    | "my_neg_4"
    | "my_5"
    | "my_neg_5"
    | "my_6"
    | "my_neg_6"
    | "my_7"
    | "my_neg_7"
    | "my_8"
    | "my_neg_8"
    | "my_9"
    | "my_neg_9"
    | "my_10"
    | "my_neg_10"
    | "my_11"
    | "my_neg_11"
    | "my_12"
    | "my_neg_12"
    | "my_16"
    | "my_neg_16"
    | "my_20"
    | "my_neg_20"
    | "my_24"
    | "my_neg_24"
    | "my_32"
    | "my_neg_32"
    | "my_40"
    | "my_neg_40"
    | "my_48"
    | "my_neg_48"
    | "my_56"
    | "my_neg_56"
    | "my_64"
    | "my_neg_64"
    | "my_72"
    | "my_neg_72"
    | "my_80"
    | "my_neg_80"
    | "my_96"
    | "my_neg_96"
    | "my_112"
    | "my_neg_112"
    | "my_128"
    | "my_neg_128"
    | "my_auto"
    | "my_px"
    | "my_neg_px"
    | "my_full"
    | "my_neg_full"
    | "my_1_2"
    | "my_neg_1_2"
    | "my_1_3"
    | "my_neg_1_3"
    | "my_2_3"
    | "my_neg_2_3"
    | "my_1_4"
    | "my_neg_1_4"
    | "my_2_4"
    | "my_neg_2_4"
    | "my_3_4"
    | "my_neg_3_4"
    | "my_1_5"
    | "my_neg_1_5"
    | "my_2_5"
    | "my_neg_2_5"
    | "my_3_5"
    | "my_neg_3_5"
    | "my_4_5"
    | "my_neg_4_5"
    | "my_1_6"
    | "my_neg_1_6"
    | "my_5_6"
    | "my_neg_5_6"
    | "my_1_12"
    | "my_neg_1_12"
    | "mx_0"
    | "mx_neg_0"
    | "mx_0p5"
    | "mx_neg_0p5"
    | "mx_1"
    | "mx_neg_1"
    | "mx_1p5"
    | "mx_neg_1p5"
    | "mx_2"
    | "mx_neg_2"
    | "mx_2p5"
    | "mx_neg_2p5"
    | "mx_3"
    | "mx_neg_3"
    | "mx_3p5"
    | "mx_neg_3p5"
    | "mx_4"
    | "mx_neg_4"
    | "mx_5"
    | "mx_neg_5"
    | "mx_6"
    | "mx_neg_6"
    | "mx_7"
    | "mx_neg_7"
    | "mx_8"
    | "mx_neg_8"
    | "mx_9"
    | "mx_neg_9"
    | "mx_10"
    | "mx_neg_10"
    | "mx_11"
    | "mx_neg_11"
    | "mx_12"
    | "mx_neg_12"
    | "mx_16"
    | "mx_neg_16"
    | "mx_20"
    | "mx_neg_20"
    | "mx_24"
    | "mx_neg_24"
    | "mx_32"
    | "mx_neg_32"
    | "mx_40"
    | "mx_neg_40"
    | "mx_48"
    | "mx_neg_48"
    | "mx_56"
    | "mx_neg_56"
    | "mx_64"
    | "mx_neg_64"
    | "mx_72"
    | "mx_neg_72"
    | "mx_80"
    | "mx_neg_80"
    | "mx_96"
    | "mx_neg_96"
    | "mx_112"
    | "mx_neg_112"
    | "mx_128"
    | "mx_neg_128"
    | "mx_auto"
    | "mx_px"
    | "mx_neg_px"
    | "mx_full"
    | "mx_neg_full"
    | "mx_1_2"
    | "mx_neg_1_2"
    | "mx_1_3"
    | "mx_neg_1_3"
    | "mx_2_3"
    | "mx_neg_2_3"
    | "mx_1_4"
    | "mx_neg_1_4"
    | "mx_2_4"
    | "mx_neg_2_4"
    | "mx_3_4"
    | "mx_neg_3_4"
    | "mx_1_5"
    | "mx_neg_1_5"
    | "mx_2_5"
    | "mx_neg_2_5"
    | "mx_3_5"
    | "mx_neg_3_5"
    | "mx_4_5"
    | "mx_neg_4_5"
    | "mx_1_6"
    | "mx_neg_1_6"
    | "mx_5_6"
    | "mx_neg_5_6"
    | "mx_1_12"
    | "mx_neg_1_12"
    | "ml_0"
    | "ml_neg_0"
    | "ml_0p5"
    | "ml_neg_0p5"
    | "ml_1"
    | "ml_neg_1"
    | "ml_1p5"
    | "ml_neg_1p5"
    | "ml_2"
    | "ml_neg_2"
    | "ml_2p5"
    | "ml_neg_2p5"
    | "ml_3"
    | "ml_neg_3"
    | "ml_3p5"
    | "ml_neg_3p5"
    | "ml_4"
    | "ml_neg_4"
    | "ml_5"
    | "ml_neg_5"
    | "ml_6"
    | "ml_neg_6"
    | "ml_7"
    | "ml_neg_7"
    | "ml_8"
    | "ml_neg_8"
    | "ml_9"
    | "ml_neg_9"
    | "ml_10"
    | "ml_neg_10"
    | "ml_11"
    | "ml_neg_11"
    | "ml_12"
    | "ml_neg_12"
    | "ml_16"
    | "ml_neg_16"
    | "ml_20"
    | "ml_neg_20"
    | "ml_24"
    | "ml_neg_24"
    | "ml_32"
    | "ml_neg_32"
    | "ml_40"
    | "ml_neg_40"
    | "ml_48"
    | "ml_neg_48"
    | "ml_56"
    | "ml_neg_56"
    | "ml_64"
    | "ml_neg_64"
    | "ml_72"
    | "ml_neg_72"
    | "ml_80"
    | "ml_neg_80"
    | "ml_96"
    | "ml_neg_96"
    | "ml_112"
    | "ml_neg_112"
    | "ml_128"
    | "ml_neg_128"
    | "ml_auto"
    | "ml_px"
    | "ml_neg_px"
    | "ml_full"
    | "ml_neg_full"
    | "ml_1_2"
    | "ml_neg_1_2"
    | "ml_1_3"
    | "ml_neg_1_3"
    | "ml_2_3"
    | "ml_neg_2_3"
    | "ml_1_4"
    | "ml_neg_1_4"
    | "ml_2_4"
    | "ml_neg_2_4"
    | "ml_3_4"
    | "ml_neg_3_4"
    | "ml_1_5"
    | "ml_neg_1_5"
    | "ml_2_5"
    | "ml_neg_2_5"
    | "ml_3_5"
    | "ml_neg_3_5"
    | "ml_4_5"
    | "ml_neg_4_5"
    | "ml_1_6"
    | "ml_neg_1_6"
    | "ml_5_6"
    | "ml_neg_5_6"
    | "ml_1_12"
    | "ml_neg_1_12"
    | "mr_0"
    | "mr_neg_0"
    | "mr_0p5"
    | "mr_neg_0p5"
    | "mr_1"
    | "mr_neg_1"
    | "mr_1p5"
    | "mr_neg_1p5"
    | "mr_2"
    | "mr_neg_2"
    | "mr_2p5"
    | "mr_neg_2p5"
    | "mr_3"
    | "mr_neg_3"
    | "mr_3p5"
    | "mr_neg_3p5"
    | "mr_4"
    | "mr_neg_4"
    | "mr_5"
    | "mr_neg_5"
    | "mr_6"
    | "mr_neg_6"
    | "mr_7"
    | "mr_neg_7"
    | "mr_8"
    | "mr_neg_8"
    | "mr_9"
    | "mr_neg_9"
    | "mr_10"
    | "mr_neg_10"
    | "mr_11"
    | "mr_neg_11"
    | "mr_12"
    | "mr_neg_12"
    | "mr_16"
    | "mr_neg_16"
    | "mr_20"
    | "mr_neg_20"
    | "mr_24"
    | "mr_neg_24"
    | "mr_32"
    | "mr_neg_32"
    | "mr_40"
    | "mr_neg_40"
    | "mr_48"
    | "mr_neg_48"
    | "mr_56"
    | "mr_neg_56"
    | "mr_64"
    | "mr_neg_64"
    | "mr_72"
    | "mr_neg_72"
    | "mr_80"
    | "mr_neg_80"
    | "mr_96"
    | "mr_neg_96"
    | "mr_112"
    | "mr_neg_112"
    | "mr_128"
    | "mr_neg_128"
    | "mr_auto"
    | "mr_px"
    | "mr_neg_px"
    | "mr_full"
    | "mr_neg_full"
    | "mr_1_2"
    | "mr_neg_1_2"
    | "mr_1_3"
    | "mr_neg_1_3"
    | "mr_2_3"
    | "mr_neg_2_3"
    | "mr_1_4"
    | "mr_neg_1_4"
    | "mr_2_4"
    | "mr_neg_2_4"
    | "mr_3_4"
    | "mr_neg_3_4"
    | "mr_1_5"
    | "mr_neg_1_5"
    | "mr_2_5"
    | "mr_neg_2_5"
    | "mr_3_5"
    | "mr_neg_3_5"
    | "mr_4_5"
    | "mr_neg_4_5"
    | "mr_1_6"
    | "mr_neg_1_6"
    | "mr_5_6"
    | "mr_neg_5_6"
    | "mr_1_12"
    | "mr_neg_1_12"
    | "p_0"
    | "p_neg_0"
    | "p_0p5"
    | "p_neg_0p5"
    | "p_1"
    | "p_neg_1"
    | "p_1p5"
    | "p_neg_1p5"
    | "p_2"
    | "p_neg_2"
    | "p_2p5"
    | "p_neg_2p5"
    | "p_3"
    | "p_neg_3"
    | "p_3p5"
    | "p_neg_3p5"
    | "p_4"
    | "p_neg_4"
    | "p_5"
    | "p_neg_5"
    | "p_6"
    | "p_neg_6"
    | "p_7"
    | "p_neg_7"
    | "p_8"
    | "p_neg_8"
    | "p_9"
    | "p_neg_9"
    | "p_10"
    | "p_neg_10"
    | "p_11"
    | "p_neg_11"
    | "p_12"
    | "p_neg_12"
    | "p_16"
    | "p_neg_16"
    | "p_20"
    | "p_neg_20"
    | "p_24"
    | "p_neg_24"
    | "p_32"
    | "p_neg_32"
    | "p_40"
    | "p_neg_40"
    | "p_48"
    | "p_neg_48"
    | "p_56"
    | "p_neg_56"
    | "p_64"
    | "p_neg_64"
    | "p_72"
    | "p_neg_72"
    | "p_80"
    | "p_neg_80"
    | "p_96"
    | "p_neg_96"
    | "p_112"
    | "p_neg_112"
    | "p_128"
    | "p_neg_128"
    | "p_px"
    | "p_neg_px"
    | "p_full"
    | "p_neg_full"
    | "p_1_2"
    | "p_neg_1_2"
    | "p_1_3"
    | "p_neg_1_3"
    | "p_2_3"
    | "p_neg_2_3"
    | "p_1_4"
    | "p_neg_1_4"
    | "p_2_4"
    | "p_neg_2_4"
    | "p_3_4"
    | "p_neg_3_4"
    | "p_1_5"
    | "p_neg_1_5"
    | "p_2_5"
    | "p_neg_2_5"
    | "p_3_5"
    | "p_neg_3_5"
    | "p_4_5"
    | "p_neg_4_5"
    | "p_1_6"
    | "p_neg_1_6"
    | "p_5_6"
    | "p_neg_5_6"
    | "p_1_12"
    | "p_neg_1_12"
    | "pt_0"
    | "pt_neg_0"
    | "pt_0p5"
    | "pt_neg_0p5"
    | "pt_1"
    | "pt_neg_1"
    | "pt_1p5"
    | "pt_neg_1p5"
    | "pt_2"
    | "pt_neg_2"
    | "pt_2p5"
    | "pt_neg_2p5"
    | "pt_3"
    | "pt_neg_3"
    | "pt_3p5"
    | "pt_neg_3p5"
    | "pt_4"
    | "pt_neg_4"
    | "pt_5"
    | "pt_neg_5"
    | "pt_6"
    | "pt_neg_6"
    | "pt_7"
    | "pt_neg_7"
    | "pt_8"
    | "pt_neg_8"
    | "pt_9"
    | "pt_neg_9"
    | "pt_10"
    | "pt_neg_10"
    | "pt_11"
    | "pt_neg_11"
    | "pt_12"
    | "pt_neg_12"
    | "pt_16"
    | "pt_neg_16"
    | "pt_20"
    | "pt_neg_20"
    | "pt_24"
    | "pt_neg_24"
    | "pt_32"
    | "pt_neg_32"
    | "pt_40"
    | "pt_neg_40"
    | "pt_48"
    | "pt_neg_48"
    | "pt_56"
    | "pt_neg_56"
    | "pt_64"
    | "pt_neg_64"
    | "pt_72"
    | "pt_neg_72"
    | "pt_80"
    | "pt_neg_80"
    | "pt_96"
    | "pt_neg_96"
    | "pt_112"
    | "pt_neg_112"
    | "pt_128"
    | "pt_neg_128"
    | "pt_px"
    | "pt_neg_px"
    | "pt_full"
    | "pt_neg_full"
    | "pt_1_2"
    | "pt_neg_1_2"
    | "pt_1_3"
    | "pt_neg_1_3"
    | "pt_2_3"
    | "pt_neg_2_3"
    | "pt_1_4"
    | "pt_neg_1_4"
    | "pt_2_4"
    | "pt_neg_2_4"
    | "pt_3_4"
    | "pt_neg_3_4"
    | "pt_1_5"
    | "pt_neg_1_5"
    | "pt_2_5"
    | "pt_neg_2_5"
    | "pt_3_5"
    | "pt_neg_3_5"
    | "pt_4_5"
    | "pt_neg_4_5"
    | "pt_1_6"
    | "pt_neg_1_6"
    | "pt_5_6"
    | "pt_neg_5_6"
    | "pt_1_12"
    | "pt_neg_1_12"
    | "pb_0"
    | "pb_neg_0"
    | "pb_0p5"
    | "pb_neg_0p5"
    | "pb_1"
    | "pb_neg_1"
    | "pb_1p5"
    | "pb_neg_1p5"
    | "pb_2"
    | "pb_neg_2"
    | "pb_2p5"
    | "pb_neg_2p5"
    | "pb_3"
    | "pb_neg_3"
    | "pb_3p5"
    | "pb_neg_3p5"
    | "pb_4"
    | "pb_neg_4"
    | "pb_5"
    | "pb_neg_5"
    | "pb_6"
    | "pb_neg_6"
    | "pb_7"
    | "pb_neg_7"
    | "pb_8"
    | "pb_neg_8"
    | "pb_9"
    | "pb_neg_9"
    | "pb_10"
    | "pb_neg_10"
    | "pb_11"
    | "pb_neg_11"
    | "pb_12"
    | "pb_neg_12"
    | "pb_16"
    | "pb_neg_16"
    | "pb_20"
    | "pb_neg_20"
    | "pb_24"
    | "pb_neg_24"
    | "pb_32"
    | "pb_neg_32"
    | "pb_40"
    | "pb_neg_40"
    | "pb_48"
    | "pb_neg_48"
    | "pb_56"
    | "pb_neg_56"
    | "pb_64"
    | "pb_neg_64"
    | "pb_72"
    | "pb_neg_72"
    | "pb_80"
    | "pb_neg_80"
    | "pb_96"
    | "pb_neg_96"
    | "pb_112"
    | "pb_neg_112"
    | "pb_128"
    | "pb_neg_128"
    | "pb_px"
    | "pb_neg_px"
    | "pb_full"
    | "pb_neg_full"
    | "pb_1_2"
    | "pb_neg_1_2"
    | "pb_1_3"
    | "pb_neg_1_3"
    | "pb_2_3"
    | "pb_neg_2_3"
    | "pb_1_4"
    | "pb_neg_1_4"
    | "pb_2_4"
    | "pb_neg_2_4"
    | "pb_3_4"
    | "pb_neg_3_4"
    | "pb_1_5"
    | "pb_neg_1_5"
    | "pb_2_5"
    | "pb_neg_2_5"
    | "pb_3_5"
    | "pb_neg_3_5"
    | "pb_4_5"
    | "pb_neg_4_5"
    | "pb_1_6"
    | "pb_neg_1_6"
    | "pb_5_6"
    | "pb_neg_5_6"
    | "pb_1_12"
    | "pb_neg_1_12"
    | "px_0"
    | "px_neg_0"
    | "px_0p5"
    | "px_neg_0p5"
    | "px_1"
    | "px_neg_1"
    | "px_1p5"
    | "px_neg_1p5"
    | "px_2"
    | "px_neg_2"
    | "px_2p5"
    | "px_neg_2p5"
    | "px_3"
    | "px_neg_3"
    | "px_3p5"
    | "px_neg_3p5"
    | "px_4"
    | "px_neg_4"
    | "px_5"
    | "px_neg_5"
    | "px_6"
    | "px_neg_6"
    | "px_7"
    | "px_neg_7"
    | "px_8"
    | "px_neg_8"
    | "px_9"
    | "px_neg_9"
    | "px_10"
    | "px_neg_10"
    | "px_11"
    | "px_neg_11"
    | "px_12"
    | "px_neg_12"
    | "px_16"
    | "px_neg_16"
    | "px_20"
    | "px_neg_20"
    | "px_24"
    | "px_neg_24"
    | "px_32"
    | "px_neg_32"
    | "px_40"
    | "px_neg_40"
    | "px_48"
    | "px_neg_48"
    | "px_56"
    | "px_neg_56"
    | "px_64"
    | "px_neg_64"
    | "px_72"
    | "px_neg_72"
    | "px_80"
    | "px_neg_80"
    | "px_96"
    | "px_neg_96"
    | "px_112"
    | "px_neg_112"
    | "px_128"
    | "px_neg_128"
    | "px_px"
    | "px_neg_px"
    | "px_full"
    | "px_neg_full"
    | "px_1_2"
    | "px_neg_1_2"
    | "px_1_3"
    | "px_neg_1_3"
    | "px_2_3"
    | "px_neg_2_3"
    | "px_1_4"
    | "px_neg_1_4"
    | "px_2_4"
    | "px_neg_2_4"
    | "px_3_4"
    | "px_neg_3_4"
    | "px_1_5"
    | "px_neg_1_5"
    | "px_2_5"
    | "px_neg_2_5"
    | "px_3_5"
    | "px_neg_3_5"
    | "px_4_5"
    | "px_neg_4_5"
    | "px_1_6"
    | "px_neg_1_6"
    | "px_5_6"
    | "px_neg_5_6"
    | "px_1_12"
    | "px_neg_1_12"
    | "py_0"
    | "py_neg_0"
    | "py_0p5"
    | "py_neg_0p5"
    | "py_1"
    | "py_neg_1"
    | "py_1p5"
    | "py_neg_1p5"
    | "py_2"
    | "py_neg_2"
    | "py_2p5"
    | "py_neg_2p5"
    | "py_3"
    | "py_neg_3"
    | "py_3p5"
    | "py_neg_3p5"
    | "py_4"
    | "py_neg_4"
    | "py_5"
    | "py_neg_5"
    | "py_6"
    | "py_neg_6"
    | "py_7"
    | "py_neg_7"
    | "py_8"
    | "py_neg_8"
    | "py_9"
    | "py_neg_9"
    | "py_10"
    | "py_neg_10"
    | "py_11"
    | "py_neg_11"
    | "py_12"
    | "py_neg_12"
    | "py_16"
    | "py_neg_16"
    | "py_20"
    | "py_neg_20"
    | "py_24"
    | "py_neg_24"
    | "py_32"
    | "py_neg_32"
    | "py_40"
    | "py_neg_40"
    | "py_48"
    | "py_neg_48"
    | "py_56"
    | "py_neg_56"
    | "py_64"
    | "py_neg_64"
    | "py_72"
    | "py_neg_72"
    | "py_80"
    | "py_neg_80"
    | "py_96"
    | "py_neg_96"
    | "py_112"
    | "py_neg_112"
    | "py_128"
    | "py_neg_128"
    | "py_px"
    | "py_neg_px"
    | "py_full"
    | "py_neg_full"
    | "py_1_2"
    | "py_neg_1_2"
    | "py_1_3"
    | "py_neg_1_3"
    | "py_2_3"
    | "py_neg_2_3"
    | "py_1_4"
    | "py_neg_1_4"
    | "py_2_4"
    | "py_neg_2_4"
    | "py_3_4"
    | "py_neg_3_4"
    | "py_1_5"
    | "py_neg_1_5"
    | "py_2_5"
    | "py_neg_2_5"
    | "py_3_5"
    | "py_neg_3_5"
    | "py_4_5"
    | "py_neg_4_5"
    | "py_1_6"
    | "py_neg_1_6"
    | "py_5_6"
    | "py_neg_5_6"
    | "py_1_12"
    | "py_neg_1_12"
    | "pl_0"
    | "pl_neg_0"
    | "pl_0p5"
    | "pl_neg_0p5"
    | "pl_1"
    | "pl_neg_1"
    | "pl_1p5"
    | "pl_neg_1p5"
    | "pl_2"
    | "pl_neg_2"
    | "pl_2p5"
    | "pl_neg_2p5"
    | "pl_3"
    | "pl_neg_3"
    | "pl_3p5"
    | "pl_neg_3p5"
    | "pl_4"
    | "pl_neg_4"
    | "pl_5"
    | "pl_neg_5"
    | "pl_6"
    | "pl_neg_6"
    | "pl_7"
    | "pl_neg_7"
    | "pl_8"
    | "pl_neg_8"
    | "pl_9"
    | "pl_neg_9"
    | "pl_10"
    | "pl_neg_10"
    | "pl_11"
    | "pl_neg_11"
    | "pl_12"
    | "pl_neg_12"
    | "pl_16"
    | "pl_neg_16"
    | "pl_20"
    | "pl_neg_20"
    | "pl_24"
    | "pl_neg_24"
    | "pl_32"
    | "pl_neg_32"
    | "pl_40"
    | "pl_neg_40"
    | "pl_48"
    | "pl_neg_48"
    | "pl_56"
    | "pl_neg_56"
    | "pl_64"
    | "pl_neg_64"
    | "pl_72"
    | "pl_neg_72"
    | "pl_80"
    | "pl_neg_80"
    | "pl_96"
    | "pl_neg_96"
    | "pl_112"
    | "pl_neg_112"
    | "pl_128"
    | "pl_neg_128"
    | "pl_px"
    | "pl_neg_px"
    | "pl_full"
    | "pl_neg_full"
    | "pl_1_2"
    | "pl_neg_1_2"
    | "pl_1_3"
    | "pl_neg_1_3"
    | "pl_2_3"
    | "pl_neg_2_3"
    | "pl_1_4"
    | "pl_neg_1_4"
    | "pl_2_4"
    | "pl_neg_2_4"
    | "pl_3_4"
    | "pl_neg_3_4"
    | "pl_1_5"
    | "pl_neg_1_5"
    | "pl_2_5"
    | "pl_neg_2_5"
    | "pl_3_5"
    | "pl_neg_3_5"
    | "pl_4_5"
    | "pl_neg_4_5"
    | "pl_1_6"
    | "pl_neg_1_6"
    | "pl_5_6"
    | "pl_neg_5_6"
    | "pl_1_12"
    | "pl_neg_1_12"
    | "pr_0"
    | "pr_neg_0"
    | "pr_0p5"
    | "pr_neg_0p5"
    | "pr_1"
    | "pr_neg_1"
    | "pr_1p5"
    | "pr_neg_1p5"
    | "pr_2"
    | "pr_neg_2"
    | "pr_2p5"
    | "pr_neg_2p5"
    | "pr_3"
    | "pr_neg_3"
    | "pr_3p5"
    | "pr_neg_3p5"
    | "pr_4"
    | "pr_neg_4"
    | "pr_5"
    | "pr_neg_5"
    | "pr_6"
    | "pr_neg_6"
    | "pr_7"
    | "pr_neg_7"
    | "pr_8"
    | "pr_neg_8"
    | "pr_9"
    | "pr_neg_9"
    | "pr_10"
    | "pr_neg_10"
    | "pr_11"
    | "pr_neg_11"
    | "pr_12"
    | "pr_neg_12"
    | "pr_16"
    | "pr_neg_16"
    | "pr_20"
    | "pr_neg_20"
    | "pr_24"
    | "pr_neg_24"
    | "pr_32"
    | "pr_neg_32"
    | "pr_40"
    | "pr_neg_40"
    | "pr_48"
    | "pr_neg_48"
    | "pr_56"
    | "pr_neg_56"
    | "pr_64"
    | "pr_neg_64"
    | "pr_72"
    | "pr_neg_72"
    | "pr_80"
    | "pr_neg_80"
    | "pr_96"
    | "pr_neg_96"
    | "pr_112"
    | "pr_neg_112"
    | "pr_128"
    | "pr_neg_128"
    | "pr_px"
    | "pr_neg_px"
    | "pr_full"
    | "pr_neg_full"
    | "pr_1_2"
    | "pr_neg_1_2"
    | "pr_1_3"
    | "pr_neg_1_3"
    | "pr_2_3"
    | "pr_neg_2_3"
    | "pr_1_4"
    | "pr_neg_1_4"
    | "pr_2_4"
    | "pr_neg_2_4"
    | "pr_3_4"
    | "pr_neg_3_4"
    | "pr_1_5"
    | "pr_neg_1_5"
    | "pr_2_5"
    | "pr_neg_2_5"
    | "pr_3_5"
    | "pr_neg_3_5"
    | "pr_4_5"
    | "pr_neg_4_5"
    | "pr_1_6"
    | "pr_neg_1_6"
    | "pr_5_6"
    | "pr_neg_5_6"
    | "pr_1_12"
    | "pr_neg_1_12"
    | "relative"
    | "absolute"
    | "inset_0"
    | "inset_neg_0"
    | "inset_0p5"
    | "inset_neg_0p5"
    | "inset_1"
    | "inset_neg_1"
    | "inset_1p5"
    | "inset_neg_1p5"
    | "inset_2"
    | "inset_neg_2"
    | "inset_2p5"
    | "inset_neg_2p5"
    | "inset_3"
    | "inset_neg_3"
    | "inset_3p5"
    | "inset_neg_3p5"
    | "inset_4"
    | "inset_neg_4"
    | "inset_5"
    | "inset_neg_5"
    | "inset_6"
    | "inset_neg_6"
    | "inset_7"
    | "inset_neg_7"
    | "inset_8"
    | "inset_neg_8"
    | "inset_9"
    | "inset_neg_9"
    | "inset_10"
    | "inset_neg_10"
    | "inset_11"
    | "inset_neg_11"
    | "inset_12"
    | "inset_neg_12"
    | "inset_16"
    | "inset_neg_16"
    | "inset_20"
    | "inset_neg_20"
    | "inset_24"
    | "inset_neg_24"
    | "inset_32"
    | "inset_neg_32"
    | "inset_40"
    | "inset_neg_40"
    | "inset_48"
    | "inset_neg_48"
    | "inset_56"
    | "inset_neg_56"
    | "inset_64"
    | "inset_neg_64"
    | "inset_72"
    | "inset_neg_72"
    | "inset_80"
    | "inset_neg_80"
    | "inset_96"
    | "inset_neg_96"
    | "inset_112"
    | "inset_neg_112"
    | "inset_128"
    | "inset_neg_128"
    | "inset_auto"
    | "inset_px"
    | "inset_neg_px"
    | "inset_full"
    | "inset_neg_full"
    | "inset_1_2"
    | "inset_neg_1_2"
    | "inset_1_3"
    | "inset_neg_1_3"
    | "inset_2_3"
    | "inset_neg_2_3"
    | "inset_1_4"
    | "inset_neg_1_4"
    | "inset_2_4"
    | "inset_neg_2_4"
    | "inset_3_4"
    | "inset_neg_3_4"
    | "inset_1_5"
    | "inset_neg_1_5"
    | "inset_2_5"
    | "inset_neg_2_5"
    | "inset_3_5"
    | "inset_neg_3_5"
    | "inset_4_5"
    | "inset_neg_4_5"
    | "inset_1_6"
    | "inset_neg_1_6"
    | "inset_5_6"
    | "inset_neg_5_6"
    | "inset_1_12"
    | "inset_neg_1_12"
    | "top_0"
    | "top_neg_0"
    | "top_0p5"
    | "top_neg_0p5"
    | "top_1"
    | "top_neg_1"
    | "top_1p5"
    | "top_neg_1p5"
    | "top_2"
    | "top_neg_2"
    | "top_2p5"
    | "top_neg_2p5"
    | "top_3"
    | "top_neg_3"
    | "top_3p5"
    | "top_neg_3p5"
    | "top_4"
    | "top_neg_4"
    | "top_5"
    | "top_neg_5"
    | "top_6"
    | "top_neg_6"
    | "top_7"
    | "top_neg_7"
    | "top_8"
    | "top_neg_8"
    | "top_9"
    | "top_neg_9"
    | "top_10"
    | "top_neg_10"
    | "top_11"
    | "top_neg_11"
    | "top_12"
    | "top_neg_12"
    | "top_16"
    | "top_neg_16"
    | "top_20"
    | "top_neg_20"
    | "top_24"
    | "top_neg_24"
    | "top_32"
    | "top_neg_32"
    | "top_40"
    | "top_neg_40"
    | "top_48"
    | "top_neg_48"
    | "top_56"
    | "top_neg_56"
    | "top_64"
    | "top_neg_64"
    | "top_72"
    | "top_neg_72"
    | "top_80"
    | "top_neg_80"
    | "top_96"
    | "top_neg_96"
    | "top_112"
    | "top_neg_112"
    | "top_128"
    | "top_neg_128"
    | "top_auto"
    | "top_px"
    | "top_neg_px"
    | "top_full"
    | "top_neg_full"
    | "top_1_2"
    | "top_neg_1_2"
    | "top_1_3"
    | "top_neg_1_3"
    | "top_2_3"
    | "top_neg_2_3"
    | "top_1_4"
    | "top_neg_1_4"
    | "top_2_4"
    | "top_neg_2_4"
    | "top_3_4"
    | "top_neg_3_4"
    | "top_1_5"
    | "top_neg_1_5"
    | "top_2_5"
    | "top_neg_2_5"
    | "top_3_5"
    | "top_neg_3_5"
    | "top_4_5"
    | "top_neg_4_5"
    | "top_1_6"
    | "top_neg_1_6"
    | "top_5_6"
    | "top_neg_5_6"
    | "top_1_12"
    | "top_neg_1_12"
    | "bottom_0"
    | "bottom_neg_0"
    | "bottom_0p5"
    | "bottom_neg_0p5"
    | "bottom_1"
    | "bottom_neg_1"
    | "bottom_1p5"
    | "bottom_neg_1p5"
    | "bottom_2"
    | "bottom_neg_2"
    | "bottom_2p5"
    | "bottom_neg_2p5"
    | "bottom_3"
    | "bottom_neg_3"
    | "bottom_3p5"
    | "bottom_neg_3p5"
    | "bottom_4"
    | "bottom_neg_4"
    | "bottom_5"
    | "bottom_neg_5"
    | "bottom_6"
    | "bottom_neg_6"
    | "bottom_7"
    | "bottom_neg_7"
    | "bottom_8"
    | "bottom_neg_8"
    | "bottom_9"
    | "bottom_neg_9"
    | "bottom_10"
    | "bottom_neg_10"
    | "bottom_11"
    | "bottom_neg_11"
    | "bottom_12"
    | "bottom_neg_12"
    | "bottom_16"
    | "bottom_neg_16"
    | "bottom_20"
    | "bottom_neg_20"
    | "bottom_24"
    | "bottom_neg_24"
    | "bottom_32"
    | "bottom_neg_32"
    | "bottom_40"
    | "bottom_neg_40"
    | "bottom_48"
    | "bottom_neg_48"
    | "bottom_56"
    | "bottom_neg_56"
    | "bottom_64"
    | "bottom_neg_64"
    | "bottom_72"
    | "bottom_neg_72"
    | "bottom_80"
    | "bottom_neg_80"
    | "bottom_96"
    | "bottom_neg_96"
    | "bottom_112"
    | "bottom_neg_112"
    | "bottom_128"
    | "bottom_neg_128"
    | "bottom_auto"
    | "bottom_px"
    | "bottom_neg_px"
    | "bottom_full"
    | "bottom_neg_full"
    | "bottom_1_2"
    | "bottom_neg_1_2"
    | "bottom_1_3"
    | "bottom_neg_1_3"
    | "bottom_2_3"
    | "bottom_neg_2_3"
    | "bottom_1_4"
    | "bottom_neg_1_4"
    | "bottom_2_4"
    | "bottom_neg_2_4"
    | "bottom_3_4"
    | "bottom_neg_3_4"
    | "bottom_1_5"
    | "bottom_neg_1_5"
    | "bottom_2_5"
    | "bottom_neg_2_5"
    | "bottom_3_5"
    | "bottom_neg_3_5"
    | "bottom_4_5"
    | "bottom_neg_4_5"
    | "bottom_1_6"
    | "bottom_neg_1_6"
    | "bottom_5_6"
    | "bottom_neg_5_6"
    | "bottom_1_12"
    | "bottom_neg_1_12"
    | "left_0"
    | "left_neg_0"
    | "left_0p5"
    | "left_neg_0p5"
    | "left_1"
    | "left_neg_1"
    | "left_1p5"
    | "left_neg_1p5"
    | "left_2"
    | "left_neg_2"
    | "left_2p5"
    | "left_neg_2p5"
    | "left_3"
    | "left_neg_3"
    | "left_3p5"
    | "left_neg_3p5"
    | "left_4"
    | "left_neg_4"
    | "left_5"
    | "left_neg_5"
    | "left_6"
    | "left_neg_6"
    | "left_7"
    | "left_neg_7"
    | "left_8"
    | "left_neg_8"
    | "left_9"
    | "left_neg_9"
    | "left_10"
    | "left_neg_10"
    | "left_11"
    | "left_neg_11"
    | "left_12"
    | "left_neg_12"
    | "left_16"
    | "left_neg_16"
    | "left_20"
    | "left_neg_20"
    | "left_24"
    | "left_neg_24"
    | "left_32"
    | "left_neg_32"
    | "left_40"
    | "left_neg_40"
    | "left_48"
    | "left_neg_48"
    | "left_56"
    | "left_neg_56"
    | "left_64"
    | "left_neg_64"
    | "left_72"
    | "left_neg_72"
    | "left_80"
    | "left_neg_80"
    | "left_96"
    | "left_neg_96"
    | "left_112"
    | "left_neg_112"
    | "left_128"
    | "left_neg_128"
    | "left_auto"
    | "left_px"
    | "left_neg_px"
    | "left_full"
    | "left_neg_full"
    | "left_1_2"
    | "left_neg_1_2"
    | "left_1_3"
    | "left_neg_1_3"
    | "left_2_3"
    | "left_neg_2_3"
    | "left_1_4"
    | "left_neg_1_4"
    | "left_2_4"
    | "left_neg_2_4"
    | "left_3_4"
    | "left_neg_3_4"
    | "left_1_5"
    | "left_neg_1_5"
    | "left_2_5"
    | "left_neg_2_5"
    | "left_3_5"
    | "left_neg_3_5"
    | "left_4_5"
    | "left_neg_4_5"
    | "left_1_6"
    | "left_neg_1_6"
    | "left_5_6"
    | "left_neg_5_6"
    | "left_1_12"
    | "left_neg_1_12"
    | "right_0"
    | "right_neg_0"
    | "right_0p5"
    | "right_neg_0p5"
    | "right_1"
    | "right_neg_1"
    | "right_1p5"
    | "right_neg_1p5"
    | "right_2"
    | "right_neg_2"
    | "right_2p5"
    | "right_neg_2p5"
    | "right_3"
    | "right_neg_3"
    | "right_3p5"
    | "right_neg_3p5"
    | "right_4"
    | "right_neg_4"
    | "right_5"
    | "right_neg_5"
    | "right_6"
    | "right_neg_6"
    | "right_7"
    | "right_neg_7"
    | "right_8"
    | "right_neg_8"
    | "right_9"
    | "right_neg_9"
    | "right_10"
    | "right_neg_10"
    | "right_11"
    | "right_neg_11"
    | "right_12"
    | "right_neg_12"
    | "right_16"
    | "right_neg_16"
    | "right_20"
    | "right_neg_20"
    | "right_24"
    | "right_neg_24"
    | "right_32"
    | "right_neg_32"
    | "right_40"
    | "right_neg_40"
    | "right_48"
    | "right_neg_48"
    | "right_56"
    | "right_neg_56"
    | "right_64"
    | "right_neg_64"
    | "right_72"
    | "right_neg_72"
    | "right_80"
    | "right_neg_80"
    | "right_96"
    | "right_neg_96"
    | "right_112"
    | "right_neg_112"
    | "right_128"
    | "right_neg_128"
    | "right_auto"
    | "right_px"
    | "right_neg_px"
    | "right_full"
    | "right_neg_full"
    | "right_1_2"
    | "right_neg_1_2"
    | "right_1_3"
    | "right_neg_1_3"
    | "right_2_3"
    | "right_neg_2_3"
    | "right_1_4"
    | "right_neg_1_4"
    | "right_2_4"
    | "right_neg_2_4"
    | "right_3_4"
    | "right_neg_3_4"
    | "right_1_5"
    | "right_neg_1_5"
    | "right_2_5"
    | "right_neg_2_5"
    | "right_3_5"
    | "right_neg_3_5"
    | "right_4_5"
    | "right_neg_4_5"
    | "right_1_6"
    | "right_neg_1_6"
    | "right_5_6"
    | "right_neg_5_6"
    | "right_1_12"
    | "right_neg_1_12"
    | "overflow_hidden"
    | "overflow_x_hidden"
    | "overflow_y_hidden"
    | "cursor_default"
    | "cursor_pointer"
    | "cursor_text"
    | "cursor_move"
    | "cursor_not_allowed"
    | "cursor_context_menu"
    | "cursor_crosshair"
    | "cursor_vertical_text"
    | "cursor_alias"
    | "cursor_copy"
    | "cursor_no_drop"
    | "cursor_grab"
    | "cursor_grabbing"
    | "cursor_ew_resize"
    | "cursor_ns_resize"
    | "cursor_nesw_resize"
    | "cursor_nwse_resize"
    | "cursor_col_resize"
    | "cursor_row_resize"
    | "cursor_n_resize"
    | "cursor_e_resize"
    | "cursor_s_resize"
    | "cursor_w_resize"
    | "border_0"
    | "border_1"
    | "border_2"
    | "border_3"
    | "border_4"
    | "border_5"
    | "border_6"
    | "border_7"
    | "border_8"
    | "border_9"
    | "border_10"
    | "border_11"
    | "border_12"
    | "border_16"
    | "border_20"
    | "border_24"
    | "border_32"
    | "border_t_0"
    | "border_t_1"
    | "border_t_2"
    | "border_t_3"
    | "border_t_4"
    | "border_t_5"
    | "border_t_6"
    | "border_t_7"
    | "border_t_8"
    | "border_t_9"
    | "border_t_10"
    | "border_t_11"
    | "border_t_12"
    | "border_t_16"
    | "border_t_20"
    | "border_t_24"
    | "border_t_32"
    | "border_b_0"
    | "border_b_1"
    | "border_b_2"
    | "border_b_3"
    | "border_b_4"
    | "border_b_5"
    | "border_b_6"
    | "border_b_7"
    | "border_b_8"
    | "border_b_9"
    | "border_b_10"
    | "border_b_11"
    | "border_b_12"
    | "border_b_16"
    | "border_b_20"
    | "border_b_24"
    | "border_b_32"
    | "border_r_0"
    | "border_r_1"
    | "border_r_2"
    | "border_r_3"
    | "border_r_4"
    | "border_r_5"
    | "border_r_6"
    | "border_r_7"
    | "border_r_8"
    | "border_r_9"
    | "border_r_10"
    | "border_r_11"
    | "border_r_12"
    | "border_r_16"
    | "border_r_20"
    | "border_r_24"
    | "border_r_32"
    | "border_l_0"
    | "border_l_1"
    | "border_l_2"
    | "border_l_3"
    | "border_l_4"
    | "border_l_5"
    | "border_l_6"
    | "border_l_7"
    | "border_l_8"
    | "border_l_9"
    | "border_l_10"
    | "border_l_11"
    | "border_l_12"
    | "border_l_16"
    | "border_l_20"
    | "border_l_24"
    | "border_l_32"
    | "border_x_0"
    | "border_x_1"
    | "border_x_2"
    | "border_x_3"
    | "border_x_4"
    | "border_x_5"
    | "border_x_6"
    | "border_x_7"
    | "border_x_8"
    | "border_x_9"
    | "border_x_10"
    | "border_x_11"
    | "border_x_12"
    | "border_x_16"
    | "border_x_20"
    | "border_x_24"
    | "border_x_32"
    | "border_y_0"
    | "border_y_1"
    | "border_y_2"
    | "border_y_3"
    | "border_y_4"
    | "border_y_5"
    | "border_y_6"
    | "border_y_7"
    | "border_y_8"
    | "border_y_9"
    | "border_y_10"
    | "border_y_11"
    | "border_y_12"
    | "border_y_16"
    | "border_y_20"
    | "border_y_24"
    | "border_y_32"
    | "shadow_none"
    | "shadow_2xs"
    | "shadow_xs"
    | "shadow_sm"
    | "shadow_md"
    | "shadow_lg"
    | "shadow_xl"
    | "shadow_2xl"
    | "block"
    | "flex"
    | "grid"
    | "hidden"
    | "whitespace_normal"
    | "whitespace_nowrap"
    | "text_ellipsis"
    | "text_ellipsis_start"
    | "text_ellipsis_middle"
    | "text_left"
    | "text_center"
    | "text_right"
    | "truncate"
    | "flex_col"
    | "flex_col_reverse"
    | "flex_row"
    | "flex_row_reverse"
    | "flex_1"
    | "flex_auto"
    | "flex_initial"
    | "flex_none"
    | "flex_grow_0"
    | "flex_grow_1"
    | "flex_shrink_0"
    | "flex_shrink_1"
    | "flex_wrap"
    | "flex_wrap_reverse"
    | "flex_nowrap"
    | "items_start"
    | "items_end"
    | "items_center"
    | "items_baseline"
    | "items_stretch"
    | "self_start"
    | "self_end"
    | "self_flex_start"
    | "self_flex_end"
    | "self_center"
    | "self_baseline"
    | "self_stretch"
    | "justify_start"
    | "justify_end"
    | "justify_center"
    | "justify_between"
    | "justify_around"
    | "justify_evenly"
    | "content_normal"
    | "content_center"
    | "content_start"
    | "content_end"
    | "content_between"
    | "content_around"
    | "content_evenly"
    | "content_stretch"
    | "aspect_square"
    | "border_dashed"
    | "text_xs"
    | "text_sm"
    | "text_base"
    | "text_lg"
    | "text_xl"
    | "text_2xl"
    | "text_3xl"
    | "italic"
    | "not_italic"
    | "underline"
    | "line_through"
    | "text_decoration_none"
    | "text_decoration_solid"
    | "text_decoration_wavy"
    | "text_decoration_0"
    | "text_decoration_1"
    | "text_decoration_2"
    | "text_decoration_4"
    | "text_decoration_8"
    | "col_start_auto"
    | "col_end_auto"
    | "col_span_full"
    | "row_start_auto"
    | "row_end_auto"
    | "row_span_full"
    | "h_flex"
    | "v_flex"
    | "debug_red"
    | "debug_blue"
    | "debug_yellow"
    | "debug_green"
    | "debug_pink"
    | "font_thin"
    | "font_extralight"
    | "font_light"
    | "font_normal"
    | "font_medium"
    | "font_semibold"
    | "font_bold"
    | "font_extrabold"
    | "font_black";
  export type NullaryStyles = { [Method in NullaryStyle]: () => Element };
  /** An element of the render that built it: chain its methods, pass it to `child`. */
  export interface Element extends NullaryStyles {
    /** Sets the width. */
    w(v: Length): Element;
    /** Sets the height. */
    h(v: Length): Element;
    /** Sets the width and the height together. */
    size(v: Length): Element;
    /** Sets the minimum width. */
    min_w(v: Length): Element;
    /** Sets the minimum height. */
    min_h(v: Length): Element;
    /** Sets the minimum width and height together. */
    min_size(v: Length): Element;
    /** Sets the maximum width. */
    max_w(v: Length): Element;
    /** Sets the maximum height. */
    max_h(v: Length): Element;
    /** Sets the maximum width and height together. */
    max_size(v: Length): Element;
    /** Sets the padding on all four sides. */
    p(v: DefiniteLength): Element;
    /** Sets the padding on the left and right. */
    px(v: DefiniteLength): Element;
    /** Sets the padding on the top and bottom. */
    py(v: DefiniteLength): Element;
    /** Sets the padding on the top. */
    pt(v: DefiniteLength): Element;
    /** Sets the padding on the bottom. */
    pb(v: DefiniteLength): Element;
    /** Sets the padding on the left. */
    pl(v: DefiniteLength): Element;
    /** Sets the padding on the right. */
    pr(v: DefiniteLength): Element;
    /** Sets the margin on all four sides. */
    m(v: Length): Element;
    /** Sets the margin on the left and right. */
    mx(v: Length): Element;
    /** Sets the margin on the top and bottom. */
    my(v: Length): Element;
    /** Sets the margin on the top. */
    mt(v: Length): Element;
    /** Sets the margin on the bottom. */
    mb(v: Length): Element;
    /** Sets the margin on the left. */
    ml(v: Length): Element;
    /** Sets the margin on the right. */
    mr(v: Length): Element;
    /** Sets all four offsets of a positioned element. */
    inset(v: Length): Element;
    /** Sets the top offset of a positioned element. */
    top(v: Length): Element;
    /** Sets the bottom offset of a positioned element. */
    bottom(v: Length): Element;
    /** Sets the left offset of a positioned element. */
    left(v: Length): Element;
    /** Sets the right offset of a positioned element. */
    right(v: Length): Element;
    /** Sets the gap between children on both axes. */
    gap(v: DefiniteLength): Element;
    /** Sets the gap between columns. */
    gap_x(v: DefiniteLength): Element;
    /** Sets the gap between rows. */
    gap_y(v: DefiniteLength): Element;
    /** Sets how much of the free space this child takes, 1 when left out. */
    flex_grow(v?: number): Element;
    /** Sets how readily this child gives space back, 1 when left out. */
    flex_shrink(v?: number): Element;
    /** Sets the size this child starts from before growing or shrinking. */
    flex_basis(v: Length): Element;
    /** Sets the background. */
    bg(v: Color | Background): Element;
    /** Sets the text color, which children inherit. */
    text_color(v: Color): Element;
    /** Sets the background painted behind the text itself. */
    text_bg(v: Color): Element;
    /** Sets the font size. */
    text_size(v: AbsoluteLength): Element;
    /** Sets the font family. */
    font_family(v: string): Element;
    /** Sets the font weight, 100 to 900. */
    font_weight(v: number): Element;
    /** Sets the line height. A bare number is a multiplier (`1.45`), a string a length. */
    line_height(v: number | DefiniteLength): Element;
    /** Sets the opacity of the element and everything in it, from 0 to 1. */
    opacity(v: number): Element;
    /** Sets the border width on all four sides. Draws nothing without a color. */
    border(v: AbsoluteLength): Element;
    /** Sets the border width on the top. */
    border_t(v: AbsoluteLength): Element;
    /** Sets the border width on the bottom. */
    border_b(v: AbsoluteLength): Element;
    /** Sets the border width on the left. */
    border_l(v: AbsoluteLength): Element;
    /** Sets the border width on the right. */
    border_r(v: AbsoluteLength): Element;
    /** Sets the border width on the left and right. */
    border_x(v: AbsoluteLength): Element;
    /** Sets the border width on the top and bottom. */
    border_y(v: AbsoluteLength): Element;
    /** Sets the border color. Draws nothing without a width. */
    border_color(v: Color): Element;
    /** Sets the corner radius on all four corners. */
    rounded(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the two top corners. */
    rounded_t(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the two bottom corners. */
    rounded_b(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the two left corners. */
    rounded_l(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the two right corners. */
    rounded_r(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the top-left corner. */
    rounded_tl(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the top-right corner. */
    rounded_tr(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the bottom-left corner. */
    rounded_bl(v: AbsoluteLength | RadiusToken): Element;
    /** Sets the corner radius on the bottom-right corner. */
    rounded_br(v: AbsoluteLength | RadiusToken): Element;
    /** Adds one child. A string is text; an `Entity` from `cx.new` mounts a retained view. */
    child(child: Element | Entity | string | number | boolean): Element;
    /** Adds several children, in order. */
    children(children: Array<Element | Entity | string | number | boolean>): Element;
    /** A stable name for this element, its identity across renders. */
    id(id: string | number): Element;
    /** The key-binding context this element and its subtree sit in. */
    key_context(context: string): Element;
    /** Tracks a focus handle from `cx.focus_handle()`, so it can focus this element. */
    track_focus(handle: FocusHandle): Element;
    /** Where this element sits in the Tab order; also makes it a tab stop. */
    tab_index(index: number): Element;
    /** Whether Tab can land on this element. */
    tab_stop(value: boolean): Element;
    /** Lets a `ScrollHandle` read and drive this element's scroll position. */
    track_scroll(handle: ScrollHandle): Element;
    /** Scrolls both axes for overflowing children. */
    overflow_scroll(): Element;
    /** Scrolls horizontally for overflowing children. */
    overflow_x_scroll(): Element;
    /** Scrolls vertically for overflowing children. */
    overflow_y_scroll(): Element;
    /** The label a hover shows over this element. */
    tooltip(text: string): Element;
    /** What this element announces itself as. */
    role(name: string): Element;
    /** What a screen reader announces. */
    aria_label(label: string): Element;
    /** Same as `aria_label`. */
    accessibility_label(label: string): Element;
    /** Supplementary text a screen reader announces after the label. */
    aria_description(text: string): Element;
    /** The selected state, announced. */
    aria_selected(value: boolean): Element;
    /** The expanded state, announced. */
    aria_expanded(value: boolean): Element;
    /** The heading level, announced. */
    aria_level(level: number): Element;
    /** Announces this element as focused while an ancestor holds the keyboard. */
    aria_active_descendant(): Element;
    /** Makes the element focusable without tracking a handle. */
    focusable(value?: boolean): Element;
    /** Blocks the mouse from reaching elements behind this one. */
    occlude(): Element;
    /** Names a group for `group_hover` of descendants. */
    group(name: string): Element;
    /** `handler(event, cx)` on a click, or keyboard activation. */
    on_click(handler: (event: ClickEvent, cx: Context) => void): Element;
    /** `handler(hovered, cx)` when the pointer enters and leaves. */
    on_hover(handler: (event: boolean, cx: Context) => void): Element;
    /** `handler(event, cx)` when `button` is pressed over this element. */
    on_mouse_down(button: MouseButton, handler: (event: MouseButtonEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` when `button` is released over this element. */
    on_mouse_up(button: MouseButton, handler: (event: MouseButtonEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` on a press anywhere outside this element. */
    on_mouse_down_out(handler: (event: MouseButtonEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` while the pointer moves over this element. */
    on_mouse_move(handler: (event: MouseMoveEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` on wheel and trackpad scrolling over this element. */
    on_scroll_wheel(handler: (event: ScrollWheelEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` on a key press while this element or a descendant has focus. */
    on_key_down(handler: (event: KeyEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` on a key release while this element or a descendant has focus. */
    on_key_up(handler: (event: KeyEvent, cx: Context) => void): Element;
    /** `handler(event, cx)` when a modifier key changes while focused. */
    on_modifiers_changed(handler: (event: ModifiersChangedEvent, cx: Context) => void): Element;
    /**
     * `handler(event, cx)` when the action named `action` is dispatched to this element or
     * inside it, by a key binding (`cx.bind_keys`) or `window.dispatch_action`.
     */
    on_action(action: string, handler: (event: ActionEvent, cx: Context) => void): Element;
    /** Passes this element to `transform` and returns what it returns. */
    map<T>(transform: (element: Element) => T): T;
    /** Applies `branch` only when `condition` is truthy. */
    when(condition: unknown, branch: (element: Element) => Element): Element;
    /** Applies `branch` only when `value` is not null or undefined, passing it along. */
    when_some<T>(value: T | null | undefined, branch: (element: Element, value: T) => Element): Element;
    /** Styles while hovered. */
    hover(style: (style: Element) => Element): Element;
    /** Styles while pressed. */
    active(style: (style: Element) => Element): Element;
    /** Styles while focused. */
    focus(style: (style: Element) => Element): Element;
    /** Styles while something inside is focused. */
    in_focus(style: (style: Element) => Element): Element;
    /** Styles while focused from the keyboard. */
    focus_visible(style: (style: Element) => Element): Element;
    /** Styles while an ancestor with `group(name)` is hovered. */
    group_hover(group: string, style: (style: Element) => Element): Element;
    /** Sets the probe's label. */
    label(text: string): Element;
    /** The visible label of a button, check, switch, radio, toggle, color picker or field. */
    label(text: string): Element;
    /** An icon of an `InputGroupButton`, by asset path. */
    icon(path: string): Element;
    /** Concise hover help. */
    tooltip(text: string): Element;
    /** The controlled value of a `Checkbox`, `Switch`, `Radio` or `Toggle`. */
    checked(value: boolean): Element;
    /** The controlled state of a `corona/base` `Toggle`: a button that stays down. */
    pressed(value: boolean): Element;
    /**
     * `handler(value, cx)` with the new value: `checked` of a check, switch, radio or toggle,
     * the index of a `RadioGroup`, the stars of a `Rating`. The script owns the value.
     */
    on_change(handler: (event: any, cx: Context) => void): Element;
    /** Blocks interaction and draws the control disabled. */
    disabled(value: boolean): Element;
    /** The selected state of a `Button`. */
    selected(value: boolean): Element;
    /** Shows a spinner and blocks clicks. */
    loading(value: boolean): Element;
    /** The semantic control size. */
    size(size: "xsmall" | "small" | "medium" | "large"): Element;
    /** The outlined look of a button, toggle or keycap. */
    outline(): Element;
    /** The primary action look. */
    primary(): Element;
    /** The secondary look of a `Button`, or the muted secondary text of a `Label`. */
    secondary(text?: string): Element;
    /** The destructive action look. */
    danger(): Element;
    /** The success look. */
    success(): Element;
    /** The warning look. */
    warning(): Element;
    /** The quiet look of a button. */
    ghost(): Element;
    /** The look of a link. */
    link(): Element;
    /** Tighter padding. */
    compact(): Element;
    /** A native button variant of an `InputGroupButton`. */
    variant(variant: "default" | "primary" | "secondary" | "danger" | "warning" | "success" | "info" | "ghost" | "link" | "text"): Element;
    /** The absolute HTTP(S) URL a `Link` opens. */
    href(url: string): Element;
    /** The checked radio of a `RadioGroup`, by index. */
    selected_index(index: number): Element;
    /** The orientation a `corona/base` `RadioGroup` or `ToggleGroup` announces. */
    axis(axis: "horizontal" | "vertical"): Element;
    /** This item's one-based position and its collection's size, announced. */
    set_position(position: number, size: number): Element;
    /** Draws a `Label`'s text as a password. */
    masked(value: boolean): Element;
    /** Highlights the matches of `text` in a `Label`. */
    highlights(text: string): Element;
    /** Whether a keycap or text area draws its frame. */
    appearance(value: boolean): Element;
    /**
     * What a `Clipboard` copies, the stars of a `Rating`, or the row id a `Select` shows as
     * selected; an id without a row selects nothing.
     */
    value(value: string | number): Element;
    /** How many stars a `Rating` has. */
    max(count: number): Element;
    /** The accent color of a `Rating`. */
    color(color: Color): Element;
    /** Draws a `Textarea`'s border. */
    bordered(value: boolean): Element;
    /** Allows selecting and copying but no edits. */
    readonly(value: boolean): Element;
    /** The prompt shown while empty. */
    placeholder(text: string): Element;
    /** The prompt of a `Combobox`'s search field. */
    search_placeholder(text: string): Element;
    /** Shows a search field above a `Combobox`'s items; takes effect from the next frame. */
    searchable(value: boolean): Element;
    /** The width of a `Select`'s or `Combobox`'s menu, in pixels. */
    menu_width(width: number): Element;
    /** Splits an `OtpInput`'s cells into this many groups. */
    groups(count: number): Element;
    /** A vertical `Slider`. */
    vertical(): Element;
    /** Fills a `Slider` from the other end. */
    reverse(): Element;
    /** Draws an `InputGroup` as invalid. */
    invalid(value: boolean): Element;
    /** Draws the focus ring of an `InputGroup`. */
    focus_ring(value: boolean): Element;
    /** Supporting text under a `Field`'s control. */
    description(text: string): Element;
    /** Marks a `Field` as required. */
    required(value: boolean): Element;
    /** Indents an unlabeled `Field` of a horizontal form like the labeled ones. */
    label_indent(value: boolean): Element;
    /** How many grid columns a `Field` spans. */
    col_span(span: number): Element;
    /** Where a `Field` aligns its control. */
    align(to: string | number): Element;
    /** How many columns a `Form` lays its fields out in. */
    columns(count: number): Element;
    /** The label width of a horizontal `Form`, in pixels. */
    label_width(width: number): Element;
    /**
     * The text control of an `InputGroup` (`InputGroupInput` or `InputGroupTextarea`), or
     * the editor slot of a `corona/base` `NumberInput`.
     */
    input(control: Element | Entity | string | number | boolean): Element;
    /** Appends an `InputGroupAddon` to an `InputGroup`. */
    addon(addon: Element | Entity | string | number | boolean): Element;
    /** Names the control for accessibility. */
    aria_label(label: string): Element;
    /** What a screen reader announces. */
    accessibility_label(label: string): Element;
    /** Whether Tab lands on a `Radio`. */
    tab_stop(value: boolean): Element;
    /**
     * Renders each inline token: `render(token, cx)` answers its element, `null` for the
     * default chip.
     */
    token(render: (...args: any[]) => unknown): Element;
    /** `listener(event, cx)` after a click on a token that nothing consumed. */
    on_token_click(listener: (event: import("corona/component").InlineTokenClickEvent, cx: Context) => void): Element;
    /** `listener(event, cx)` when the pointer enters or leaves a token. */
    on_token_hover(listener: (event: import("corona/component").InlineTokenHoverEvent, cx: Context) => void): Element;
    /** The kind of text an `Input` holds, for autofill and keyboards. */
    content_type(kind: "name" | "name_prefix" | "given_name" | "middle_name" | "family_name" | "name_suffix" | "nickname" | "job_title" | "organization_name" | "location" | "full_street_address" | "street_address_line1" | "street_address_line2" | "address_city" | "address_state" | "address_city_and_state" | "sublocality" | "country_name" | "postal_code" | "telephone_number" | "email_address" | "url" | "credit_card_number" | "credit_card_name" | "credit_card_given_name" | "credit_card_middle_name" | "credit_card_family_name" | "credit_card_security_code" | "credit_card_expiration" | "credit_card_expiration_month" | "credit_card_expiration_year" | "credit_card_type" | "username" | "password" | "new_password" | "one_time_code" | "shipment_tracking_number" | "flight_number" | "date_time" | "birthdate" | "birthdate_day" | "birthdate_month" | "birthdate_year" | "cellular_eid" | "cellular_imei"): Element;
    /** A developer-assigned accessibility identifier. */
    accessibility_id(id: string): Element;
    /** The number of rows a `Textarea` shows, on its state. */
    rows(rows: number): Element;
    /** Grows a `Textarea` between two row counts, on its state. */
    auto_grow(min_rows: number, max_rows: number): Element;
    /** `handler(event, cx)` on activation. */
    on_click(handler: (event: ClickEvent, cx: Context) => void): Element;
    /** Whether a `corona/base` `Select`, `Combobox` or `DatePicker` is showing its list. */
    open(value: boolean): Element;
    /** `handler(open, cx)` when something other than the script opened or closed it. */
    on_open_change(handler: (event: boolean, cx: Context) => void): Element;
    /** `handler(_, cx)` on Enter in an open `corona/base` `Select` or `Combobox`. */
    on_confirm(handler: (event: {}, cx: Context) => void): Element;
    /** `handler(_, cx)` on Escape in an open `corona/base` `Select` or `Combobox`. */
    on_dismiss(handler: (event: {}, cx: Context) => void): Element;
    /** The handle a `corona/base` `Select` or `Combobox` moves the keyboard to when it opens. */
    content_focus_handle(handle: FocusHandle): Element;
    /** Which thumb of a range slider a `SliderThumb` is. */
    start(value: boolean): Element;
    /** How the filled part of a `SliderIndicator` looks: `declare(el)` styles it. */
    range_style(declare: (...args: any[]) => unknown): Element;
    /** How every cell of a `corona/base` `OtpInput` looks. */
    cell_style(declare: (...args: any[]) => unknown): Element;
    /** What the cell the next digit lands in adds to `cell_style`. */
    cell_active_style(declare: (...args: any[]) => unknown): Element;
    /** The blinking mark in that cell while it is empty. */
    caret_style(declare: (...args: any[]) => unknown): Element;
    /** The look of a `corona/base` `NumberInput`'s decrement button. */
    decrement_button(look: Element | Entity | string | number | boolean): Element;
    /** The look of a `corona/base` `NumberInput`'s increment button. */
    increment_button(look: Element | Entity | string | number | boolean): Element;
    /** Stacks both step buttons to the right of the text. */
    controls_right(): Element;
    /** `handler(action, cx)` on a step; replaces the built-in stepping. */
    on_step(handler: (event: "increment" | "decrement", cx: Context) => void): Element;
    /** Whether a `Tab`, sidebar row, header or footer shows as selected. */
    selected(value: boolean): Element;
    /** Disables a tab, accordion part, step, pagination, menu or command item. */
    disabled(value: boolean): Element;
    /** A `Tab`'s one-based place among `size` tabs, announced as "tab 2 of 5". */
    set_position(position: number, size: number): Element;
    /** Whether an accordion item or a collapsible is open. */
    open(value: boolean): Element;
    /** An `AccordionItem`'s `AccordionHeader`, or a `Sidebar`'s header. */
    header(el: Element | Entity | string | number | boolean): Element;
    /** A `Sidebar`'s footer. */
    footer(el: Element | Entity | string | number | boolean): Element;
    /** An `AccordionItem`'s `AccordionPanel`. */
    panel(element: Element | Entity | string | number | boolean): Element;
    /** What revealing a collapsible shows. */
    content(el: Element | Entity | string | number | boolean): Element;
    /** A stable identity for a collapsible's reveal motion. */
    motion_id(id: string | number): Element;
    /** An `AccordionHeader`'s announced heading level, 3 by default. */
    aria_level(level: number): Element;
    /** Keeps a shut `AccordionPanel` in the tree, with its scroll position and focus. */
    keep_mounted(value?: boolean): Element;
    /**
     * `handler(value, cx)`: an `AccordionTrigger`'s requested open state, or the index a
     * `TabBar` or `Stepper`, or the page a `Pagination`, asks for.
     */
    on_change(handler: (event: any, cx: Context) => void): Element;
    /** A resizable panel's initial size in pixels, or a component's semantic size. */
    size(size: number | "xsmall" | "small" | "medium" | "large"): Element;
    /** Whether a `resizable_panel()` is drawn; hidden, it keeps its place. Default `true`. */
    visible(value?: boolean): Element;
    /** How far a resizable panel may be dragged, in pixels. */
    size_range(min: number, max?: number): Element;
    /** `handler(sizes, cx)` after a drag resized the panels of a resizable group. */
    on_resize(handler: (event: number[], cx: Context) => void): Element;
    /** A `Resizable`'s direction: `"horizontal"` or `"vertical"`. */
    axis(value: string): Element;
    /** A `Resizable`'s size across its axis, in pixels. */
    cross_size(pixels: number): Element;
    /** When a `Scrollbar` shows: `"scrolling"`, `"hover"` or `"always"`. */
    mode(value: string): Element;
    /** The content size a `Scrollbar` measures its thumb against, in pixels. */
    scroll_size(width: number, height: number): Element;
    /** Makes a `Scrollbar` take its viewport from its own bounds. */
    viewport_from_layout(enabled?: boolean): Element;
    /** Which axes a `Scrollbar` or `Scroll` covers: `"vertical"`, `"horizontal"` or `"both"`. */
    scroll_axis(axis: string): Element;
    /** Lets a `VirtualListScrollHandle` drive a virtual list. */
    track_scroll(handle: import("corona/base").VirtualListScrollHandle): Element;
    /** Which item a virtual list measures for its cross-axis size. */
    with_item_to_measure_index(index: number): Element;
    /** A `Tab`'s text, or a `Separator`'s. */
    label(label: string): Element;
    /** What a `Tab` announces. */
    aria_label(label: string): Element;
    /** `handler(key, cx)` when a row of a virtual list is clicked, `key` what `get_key` gave. */
    on_item_click(handler: (event: string, cx: Context) => void): Element;
    /** `handler(key, event, cx)` on a secondary press over a row of a virtual list. */
    on_item_secondary_click(handler: (...args: any[]) => unknown): Element;
    /** The zero-based selected tab or current step. */
    selected_index(index: number): Element;
    /**
     * A `TabBar`'s look (`"tab"`, `"outline"`, `"pill"`, `"segmented"`, `"underline"`) or a
     * `GroupBox`'s (`"normal"`, `"fill"`, `"outline"`).
     */
    variant(variant: string): Element;
    /** Whether a `TabBar` lists tabs that do not fit in a menu. */
    menu(value: boolean): Element;
    /** Whether an `Accordion` keeps several items open. */
    multiple(value: boolean): Element;
    /** Whether an `Accordion` or `Command` draws its border. */
    bordered(value: boolean): Element;
    /** `handler(open_indices, cx)` after an `Accordion` item was toggled. */
    on_toggle(handler: (event: number[], cx: Context) => void): Element;
    /** An `AccordionItem`'s title row or a `GroupBox`'s title. */
    title(title: Element | Entity | string | number | boolean): Element;
    /** The current one-based page. */
    current_page(page: number): Element;
    /** How many pages there are. */
    total_pages(pages: number): Element;
    /** How many page buttons show at most. */
    visible_pages(pages: number): Element;
    /** Shows only the previous and next buttons. */
    compact(): Element;
    /** Lays a `Stepper` out top to bottom. */
    vertical(value?: boolean): Element;
    /** Centers each step's text in a horizontal `Stepper`. */
    text_center(value?: boolean): Element;
    /** A `Separator`'s line color. */
    color(color: Color): Element;
    /** Draws a `Separator` dashed. */
    dashed(): Element;
    /** Adds to a `StatusBar`'s leading side. */
    left_content(el: Element | Entity | string | number | boolean): Element;
    /** Adds to a `StatusBar`'s trailing side. */
    right_content(el: Element | Entity | string | number | boolean): Element;
    /** The side a `Sidebar` or `SidebarToggleButton` sits on: `"left"` or `"right"`. */
    side(side: string): Element;
    /** How a `Sidebar` collapses: `"icon"`, `"offcanvas"` or `"none"`. */
    collapsible(mode: string): Element;
    /** Whether a `Sidebar` is collapsed, and which icon its toggle shows. */
    collapsed(value: boolean): Element;
    /** `handler(event, cx)` when a sidebar row or toggle is pressed. */
    on_click(handler: (event: ClickEvent, cx: Context) => void): Element;
    /** Whether a sidebar row's submenu starts open. */
    default_open(value: boolean): Element;
    /** Lets a press on a sidebar row open its submenu. */
    click_to_open(value: boolean): Element;
    /** Lets a press on a sidebar row toggle its submenu. */
    click_to_toggle(value: boolean): Element;
    /** A sidebar row's icon: `"home"`, `"components"`, `"settings"`, `"archive"` or `"account"`. */
    icon(icon: string): Element;
    /** Whether a `Command` shows its search field. */
    searchable(value: boolean): Element;
    /** Whether a `Command` filters its items by the query. */
    filterable(value: boolean): Element;
    /** A `Command`'s search placeholder. */
    placeholder(text: string): Element;
    /** How tall a `Command`'s results grow, in pixels. */
    max_height(pixels: number): Element;
    /** `handler(query, cx)` after a `Command`'s query changed. */
    on_query(handler: (event: string, cx: Context) => void): Element;
    /** `handler({ section, row }, cx)` after a `Command`'s highlight moved. */
    on_select(handler: (event: { section: number; row: number }, cx: Context) => void): Element;
    /** `handler({ section, row }, cx)` after a `Command` item was confirmed. */
    on_confirm(handler: (event: { section: number; row: number }, cx: Context) => void): Element;
    /** `handler(cx)` when Escape is pressed on a `Command` with an empty query. */
    on_cancel(handler: (cx: Context) => void): Element;
    /** Another word a `CommandItem` is found by. */
    keyword(keyword: string): Element;
    /** Whether a command or native menu item shows a check. */
    checked(value: boolean): Element;
    /** The action a command or native menu item dispatches, for `on_action`. */
    action(action: string): Element;
    /** The outlined look of a `DropdownButton`. */
    outline(): Element;
    /** Where a `DropdownButton`'s menu opens. */
    menu_anchor(anchor: string): Element;
    /** Adds a `DropdownButton` menu item calling `handler(cx)`. */
    menu_item(label: string, handler: (cx: Context) => void): Element;
    /**
     * `handler(message, cx)` when the native menu cannot be shown. Showing one cannot fail
     * on this platform layer, so it is kept and never called.
     */
    on_effect_error(handler: (event: string, cx: Context) => void): Element;
    /** Adds a `DropdownMenu` item calling `handler(cx)`. */
    item(label: string, handler: (cx: Context) => void): Element;
    /**
     * A stable name for this element; a scrolling one is what a `Scrollbar` of that name
     * drives.
     */
    id(id: string | number): Element;
    /** Scrolls both axes for overflowing children. */
    overflow_scroll(): Element;
    /** Scrolls horizontally for overflowing children. */
    overflow_x_scroll(): Element;
    /** Scrolls vertically for overflowing children. */
    overflow_y_scroll(): Element;
    /** Lets a `ScrollHandle` drive this element; a `Scrollbar` of its name drives it too. */
    track_scroll(handle: ScrollHandle): Element;
    /** Scrolls both axes and paints base-layer scrollbars. */
    overflow_scrollbar(): Element;
    /** Scrolls horizontally and paints a base-layer scrollbar. */
    overflow_x_scrollbar(): Element;
    /** Scrolls vertically and paints a base-layer scrollbar. */
    overflow_y_scrollbar(): Element;
    /** The element a `corona/base` `Popover` or `HoverCard` draws while closed, and that opens it. */
    trigger(element: Element | Entity | string | number | boolean): Element;
    /** The element that opens a `corona/component` `HoverCard` when hovered. */
    trigger_element(element: Element | Entity | string | number | boolean): Element;
    /**
     * What a surface shows while open: an element of this render. A `Dialog` or `Sheet`
     * outlives the render, so it takes a fn rendering its content instead.
     */
    content(content: Element | (() => Element)): Element;
    /** Whether a `Popover` is open; set, the script holds the state through `on_open_change`. */
    open(value: boolean): Element;
    /** Whether an uncontrolled `Popover` starts open. */
    default_open(value: boolean): Element;
    /** Whether a press outside an open `Popover` closes it. Default `true`. */
    overlay_closable(value: boolean): Element;
    /** Which pointer button opens a `Popover`. Default `"left"`. */
    mouse_button(button: "left" | "right" | "middle"): Element;
    /** Which corner of a `Popover`, `HoverCard` or `Popup` is pinned to its trigger. */
    anchor(anchor: "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right" | "left_center" | "right_center"): Element;
    /** Same as `anchor`. */
    card_anchor(anchor: "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right" | "left_center" | "right_center"): Element;
    /** How long the pointer rests on a `HoverCard`'s trigger before it opens. Default 600. */
    open_delay(ms: number): Element;
    /** How long a `HoverCard` stays open after the pointer left it and its trigger. Default 300. */
    close_delay(ms: number): Element;
    /** Whether a `corona/component` `Popover` or `HoverCard` draws its surface. Default `true`. */
    appearance(value: boolean): Element;
    /** `handler(open, cx)` when a `Popover` or `HoverCard` opens or closes by itself. */
    on_open_change(handler: (event: boolean, cx: Context) => void): Element;
    /** The title of a window effect or an `Alert`. */
    title(text: string): Element;
    /** The text under an `AlertDialog`'s title. */
    description(text: string): Element;
    /** The text under a `Notification`'s title. */
    message(text: string): Element;
    /** Whether an `AlertDialog` has a cancel button. Default `false`. */
    show_cancel(value: boolean): Element;
    /** The window edge a `Sheet` slides in from. Default `"right"`. */
    placement(at: "top" | "right" | "bottom" | "left"): Element;
    /** Whether a `Notification` hides by itself. Default `true`. */
    autohide(value: boolean): Element;
    /** `handler(cx)` on a `Dialog`'s or `AlertDialog`'s OK button; the dialog closes after. */
    on_ok(handler: (cx: Context) => void): Element;
    /** `handler(cx)` on a `Dialog`'s or `AlertDialog`'s cancel button; the dialog closes after. */
    on_cancel(handler: (cx: Context) => void): Element;
    /** `handler(cx)` once a window effect closed, however it was closed. */
    on_close(handler: (cx: Context) => void): Element;
    /** `handler(cx)` on a click on a `Notification`. */
    on_click(handler: (cx: Context) => void): Element;
    /** Draws an `Alert` full width, without border, radius or title. */
    banner(): Element;
    /** Whether an `Alert` is drawn. Default `true`. */
    visible(value?: boolean): Element;
    /** An `Alert`'s semantic size. */
    size(size: "xsmall" | "small" | "medium" | "large"): Element;
    /** The kind of a `Notification`. Default `"info"`. */
    type(level: "info" | "success" | "warning" | "error"): Element;
    /**
     * The value: a `Progress` percentage (`corona/base` announces it, `corona/component` draws
     * it, `0..=100`), or the text of a `DescriptionItem`.
     */
    value(value: number | string): Element;
    /** Withdraws a `corona/base` `Progress` value from the accessibility tree: "no idea how far". */
    indeterminate(value: boolean): Element;
    /** Fills an `Avatar`'s image slot with an `AvatarImage`; it renders instead of the fallback. */
    image(image: Element): Element;
    /** Fills an `Avatar`'s fallback slot with an `AvatarFallback`, shown without an image. */
    fallback(fallback: Element): Element;
    /** A `Table`'s total number of rows, also those not rendered, as announced. */
    row_count(count: number): Element;
    /** A `Table`'s total number of columns, also those not rendered. */
    column_count(count: number): Element;
    /** The semantic size of a styled widget. */
    size(size: "xsmall" | "small" | "medium" | "large"): Element;
    /** What a screen reader announces. */
    accessibility_label(label: string): Element;
    /** How many columns a table cell spans. */
    col_span(span: number): Element;
    /** Centers a table cell's text. */
    text_center(): Element;
    /** Right-aligns a table cell's text. */
    text_right(): Element;
    /** The color of a `Badge`, `Spinner`, `Icon` or `Progress`. */
    color(color: Color): Element;
    /** A `Badge` as a dot; a `LineChart` or `RadarChart` with dots on its points. */
    dot(): Element;
    /** The number a `Badge` shows. */
    count(count: number): Element;
    /** The largest number a `Badge` shows, `max+` above. */
    max(max: number): Element;
    /** The look of a `Tag` or an `EmptyMedia`. */
    variant(variant: "primary" | "secondary" | "danger" | "success" | "warning" | "info" | "default" | "icon"): Element;
    /** Draws a `Tag` outlined. */
    outline(): Element;
    /** The quieter color of a `Skeleton`. */
    secondary(): Element;
    /** The icon a `Spinner` turns. */
    icon(icon: "loader" | "loader_circle"): Element;
    /** How a `Spinner` turns. */
    ease(ease: "linear" | "ease_in_out" | "ease_out_quint"): Element;
    /** Fills an `Empty`'s header with an `EmptyHeader`. */
    header(header: Element): Element;
    /** Fills an `Empty`'s content with an `EmptyContent`. */
    content(content: Element): Element;
    /** Fills an `EmptyHeader`'s media with an `EmptyMedia`. */
    media(media: Element): Element;
    /** Fills an `EmptyHeader`'s title with an `EmptyTitle`. */
    title(title: Element): Element;
    /** Fills an `EmptyHeader`'s description with an `EmptyDescription`. */
    description(description: Element): Element;
    /** Rotates an `Icon`. */
    rotate(radians: number): Element;
    /** Animates a `corona/component` `Progress` as busy. */
    loading(value: boolean): Element;
    /** The name an `Avatar` shows the initials of without an image. */
    name(name: string): Element;
    /** The image of an `Avatar`: an asset or an absolute path. */
    src(path: string): Element;
    /** Lays a `DescriptionList` out in a column. */
    vertical(): Element;
    /** Draws a `DescriptionList` or `DataTable` with borders. */
    bordered(value: boolean): Element;
    /** How many columns a `DescriptionList` has. */
    columns(columns: number): Element;
    /** Adds `DescriptionItem`s to a `DescriptionList`, or `TreeItem`s to a `Tree` or `TreeItem`. */
    items(items: Element[]): Element;
    /** Adds a separator row to a `DescriptionList`. */
    separator(): Element;
    /** How many columns a `DescriptionItem` spans. */
    span(span: number): Element;
    /** Whether a `TreeItem` starts expanded. */
    expanded(value: boolean): Element;
    /** Whether a `TreeItem` is disabled. */
    disabled(value: boolean): Element;
    /** Draws a chart's grid lines. */
    grid(value?: boolean): Element;
    /** Draws a `BarChart`'s label axis. */
    label_axis(value: boolean): Element;
    /** Draws a `BarChart`'s value axis. */
    value_axis(value: boolean): Element;
    /** Draws a `LineChart`'s or `AreaChart`'s x axis. */
    x_axis(value: boolean): Element;
    /** Labels every `margin`th tick of a chart's axis. */
    tick_margin(margin: number): Element;
    /** Draws a smooth curve. */
    natural(): Element;
    /** Draws straight segments. */
    linear(): Element;
    /** Draws steps. */
    step_after(): Element;
    /** A `PieChart`'s hole, in pixels: a donut. */
    inner_radius(radius: number): Element;
    /** The gap between a `PieChart`'s slices, in radians. */
    pad_angle(angle: number): Element;
    /** Labels a `PieChart`'s slices. */
    labels(value: boolean): Element;
    /** How many rings a `RadarChart`'s grid has. */
    grid_levels(levels: number): Element;
    /** How many months a `Calendar` shows side by side. */
    number_of_months(count: number): Element;
    /** Stripes a `DataTable`'s rows. */
    stripe(value: boolean): Element;
    /** When a `DataTable` shows its scrollbars. */
    scrollbar_visible(vertical: boolean, horizontal: boolean): Element;
    /** Whether a `DataTable`'s rows can be selected. */
    row_selectable(value: boolean): Element;
    /** Whether a `DataTable`'s columns can be selected. */
    column_selectable(value: boolean): Element;
    /** Whether a `DataTable`'s cells can be selected. */
    cell_selectable(value: boolean): Element;
    /** Whether a `DataTable` has a row header column. */
    row_header(value: boolean): Element;
    /** Whether a `DataTable`'s columns sort. */
    sortable(value: boolean): Element;
    /** Whether a `DataTable`'s columns resize. */
    column_resizable(value: boolean): Element;
    /** Whether a `DataTable`'s columns move. */
    column_movable(value: boolean): Element;
    /** The semantic size of an `Attachment` or `Settings`. */
    size(size: "xsmall" | "small" | "medium" | "large"): Element;
    /** The attachment's lifecycle status. */
    status(status: "pending" | "uploading" | "processing" | "failed" | "complete"): Element;
    /** The attachment's layout axis. */
    axis(axis: "horizontal" | "vertical"): Element;
    /** The sender edge a `Message` or `Bubble` sits on. */
    alignment(alignment: "start" | "end"): Element;
    /** The treatment of a `Bubble` or `Marker`. */
    variant(variant: "filled" | "secondary" | "muted" | "tinted" | "outline" | "ghost" | "destructive" | "plain" | "separator" | "border"): Element;
    /** Whether the marker shows its loading treatment. */
    loading(loading: boolean): Element;
    /** The marker's loading treatment. */
    loading_style(loading_style: "spinner" | "shimmer"): Element;
    /** A stable identity for a `ShimmerText`'s animation. */
    id(id: string | number): Element;
    /** How long one shimmer sweep takes. */
    duration_ms(duration_ms: number): Element;
    /** The highlight's half-width, relative to the text. */
    spread(spread: number): Element;
    /** Sweeps right to left. */
    reverse(reverse: boolean): Element;
    /** Sweeps once instead of looping. */
    once(once: boolean): Element;
    /** Shows the transcript's scrollbar. */
    scrollbar(scrollbar: boolean): Element;
    /** Shows the jump-to-latest button. */
    jump_button(jump_button: boolean): Element;
    /** The jump-to-latest button's label. */
    jump_button_label(label: string): Element;
    /** Disables an `Editor`, a `SettingItem`'s field, a questionnaire choice or question. */
    disabled(disabled: boolean): Element;
    /** Draws the editor's own background and border. */
    appearance(appearance: boolean): Element;
    bordered(bordered: boolean): Element;
    readonly(readonly: boolean): Element;
    /** What a screen reader announces. */
    aria_label(label: string): Element;
    /** Shows line numbers. */
    line_number(value: boolean): Element;
    syntax_highlight(value: boolean): Element;
    /** Shows a header above each file. */
    header_visible(value: boolean): Element;
    /** Tints changed lines. */
    change_background(value: boolean): Element;
    /** Wraps long lines. */
    soft_wrap(value: boolean): Element;
    /** How the start of each hunk is marked. */
    hunk_separator(separator: "metadata" | "line_info" | "simple"): Element;
    /** How changed lines are marked beside the code. */
    change_indicator(indicator: "signs" | "bars" | "none"): Element;
    /** The text under a setting's or question's title. */
    description(text: string): Element;
    /** The axis a setting's label and field are laid out along. */
    layout(axis: "horizontal" | "vertical"): Element;
    /** Search keywords matching the setting. */
    keywords(keywords: string[]): Element;
    /** The setting group's title. */
    title(text: string): Element;
    /** Renders a `SettingItem`'s field, or what follows a `SettingPage`'s title, lazily. */
    content(render: (...args: any[]) => unknown): Element;
    /** Opens the page's groups. */
    default_open(default_open: boolean): Element;
    /** Shows the reset control. */
    resettable(resettable: boolean): Element;
    /** The sidebar's width in pixels. */
    sidebar_width(pixels: number): Element;
    /** How far the sidebar can be resized, in pixels. */
    sidebar_size_range(minimum: number, maximum: number): Element;
    /** The page shown first. */
    default_selected_page(index: number): Element;
    /** Selects the choice at first. */
    default_selected(value: boolean): Element;
    /** Requires an answer; hides Skip. */
    required(value: boolean): Element;
    /** Accepts several choices. */
    multiple(value: boolean): Element;
    /** Gives each choice of the active question a letter or number shortcut. */
    shortcuts(mode: "letters" | "numbers"): Element;
    /** `handler(url, cx)` on a link click, instead of opening it. */
    on_link_click(handler: (event: string, cx: Context) => void): Element;
    /** Whether the text can be selected; on when left out. */
    selectable(value?: boolean): Element;
    /** Whether the view scrolls its content; on when left out. */
    scrollable(value?: boolean): Element;
    /** Where in its `relative()` parent the HUD sits. */
    anchor(anchor: "top_left" | "top_right" | "bottom_left" | "bottom_right" | "top_center" | "bottom_center" | "left_center" | "right_center"): Element;
    /** The frame budget in milliseconds the HUD grades against. */
    frame_budget(ms: number): Element;
    /** The tab bar above a group's displayed panel. */
    tab_bar(handler: (group: import("corona/base").DockGroup, cx: Context) => Element): Element;
    /** What a group with no displayed panel shows. */
    empty_group(handler: (group: import("corona/base").DockGroup, cx: Context) => Element | null): Element;
    /** The hint where a dragged panel would land. */
    drop_indicator(handler: (drop: import("corona/base").DockDrop, cx: Context) => Element | null): Element;
    /** One dock's chrome around its content; put `dock_content()` where its panels go. */
    dock(handler: (dock: import("corona/base").DockRegion, cx: Context) => Element | null): Element;
    /** Displays tab `index` of `group` on a click. */
    select_tab(group: import("corona/base").DockGroup, index: number): Element;
    /** Closes `panel` (an id) of `group` on a click, if the group allows it. */
    close_panel(group: import("corona/base").DockGroup, panel: number): Element;
    /** Zooms `group` in, or back out, on a click. */
    toggle_zoom(group: import("corona/base").DockGroup): Element;
    /** Makes this element the drag source of tab `index` of `group`. */
    drag_tab(group: import("corona/base").DockGroup, index: number): Element;
    /** Accepts a dragged panel into `group`, at slot `index` or appended. */
    drop_tab(group: import("corona/base").DockGroup, index?: number): Element;
    /** Opens or closes `dock` on a click. */
    toggle_dock(dock: import("corona/base").DockRegion): Element;
    /** Drags `dock`'s edge. */
    resize_dock(dock: import("corona/base").DockRegion): Element;
  }
  export type NativeElement = Element;
  /** A retained child view from `cx.new`; a child wherever a child is taken. */
  export interface Entity {
    /** Runs the child's `update(props)` and renders it again. */
    set_props(props?: Props): void;
    release(): boolean;
  }
  /** A focus target the script keeps; give it to an element with `track_focus`. */
  export interface FocusHandle {
    focus(): void;
    is_focused(): boolean;
    release(): boolean;
  }
  /** A scroll position: `track_scroll(handle)` on the scrolled element. */
  export class ScrollHandle {
    constructor();
    static new(): ScrollHandle;
    /** How far it is scrolled, negative downwards and rightwards. */
    offset(): Point;
    max_offset(): Point;
    set_offset(x: number, y: number): void;
    scroll_to_item(index: number): void;
    scroll_to_bottom(): void;
    top_item(): number;
  }
  export interface TimerHandle {
    cancel(): void;
  }
  export interface Timer {
    /** Calls `cb(cx)` once after `ms`. */
    after(ms: number, cb: (cx: Context) => void): TimerHandle;
    /** Calls `cb(cx)` every `ms`. */
    every(ms: number, cb: (cx: Context) => void): TimerHandle;
  }
  /** One entry of `cx.bind_keys`. */
  export interface KeyBinding {
    /** The chord, e.g. `"cmd-s"`, or a sequence: `"ctrl-k ctrl-s"`. */
    keystroke: string;
    /** The action it dispatches, handled by `on_action`. */
    action: string;
    /** A key-context predicate matched against `key_context`; global when left out. */
    context?: string;
  }
  /** What a view's methods, handlers, timers and tasks get. */
  export interface Context {
    /** Renders the view again, or the retained child `target`. Throws in `render`. */
    notify(target?: Entity): void;
    /** Installs key bindings, all or none; returns how many. */
    bind_keys(bindings: KeyBinding[]): number;
    /** Stops this event reaching the handlers further out. */
    stop_propagation(): void;
    /** Undoes `stop_propagation()` in the same dispatch. */
    propagate(): void;
    phase(): "render" | "event" | "task";
    /** The theme's semantic tokens. */
    theme(): import("corona/base").Theme;
    /** Opens an absolute http(s) URL with the system's handler. */
    open_url(url: string): void;
    read_from_clipboard(): string | undefined;
    write_to_clipboard(text: string): void;
    /** A focus target, made once in `init` or a handler, never in `render`. */
    focus_handle(): FocusHandle;
    /** Creates a retained child view; not in `render`. */
    new(Class: ViewClass, props?: Props): Entity;
    /** Calls `body(cx)` and returns what it returns, the promise of an async body. */
    spawn<T>(body: (cx: Context) => T): T;
    /** Resolves after `ms`. */
    sleep(ms?: number): Promise<void>;
    readonly timer: Timer;
  }
  export type AsyncContext = Context;
  /**
   * The base class of every view: default-export a subclass. `init` runs once, `render`
   * whenever the view is notified; never keep an element on the instance.
   */
  export abstract class View {
    constructor(props?: Props);
    init?(props: Props | undefined, cx: Context): void;
    /** Runs when the parent hands new props through `set_props`. */
    update?(props: Props | undefined): void;
    abstract render(cx: Context): Element | Entity | string;
  }
  export type ViewClass = new (props?: Props) => View;
  export function div(): Element;
  /** A vector icon tinted with the text color: an absolute path or an asset. */
  export function svg(path: string): Element;
  /** A full-color image: a file path or PNG, JPEG, WebP, GIF, BMP, TIFF or ICO bytes. */
  export function image(source: string | Uint8Array): Element;
  /** The visible items, `[start, end)`. */
  export interface ItemRange {
    start: number;
    end: number;
  }
  /** gpui's lazy list of rows of any height; size it, e.g. `size_full()`. */
  export function list(
    id: string,
    item_count: number,
    get_key: (index: number) => string,
    render: (index: number, cx: Context) => Element,
  ): Element;
  /** gpui's list of rows of one height, rendered a range at a time. */
  export function uniform_list(
    id: string,
    item_count: number,
    get_key: (index: number) => string,
    render: (range: ItemRange, cx: Context) => Element[],
  ): Element;
  /** Geometry from `PathBuilder.build()`, for `window.paint_path`. */
  export interface Path {}
  export interface PathBuilder {
    move_to(x: number, y: number): PathBuilder;
    line_to(x: number, y: number): PathBuilder;
    curve_to(to_x: number, to_y: number, control_x: number, control_y: number): PathBuilder;
    cubic_bezier_to(to_x: number, to_y: number, control_a_x: number, control_a_y: number, control_b_x: number, control_b_y: number): PathBuilder;
    arc_to(radius_x: number, radius_y: number, rotation: number, large_arc: boolean, sweep: boolean, to_x: number, to_y: number): PathBuilder;
    add_polygon(points: ReadonlyArray<readonly [number, number]>, closed?: boolean): PathBuilder;
    close(): PathBuilder;
    dash_array(values: readonly number[]): PathBuilder;
    build(): Path;
  }
  export const PathBuilder: {
    fill(): PathBuilder;
    stroke(width: number): PathBuilder;
  };
  export interface BackgroundStop {}
  export interface Background {
    opacity(factor: number): Background;
    color_space(space: "srgb" | "oklab"): Background;
  }
  export const Background: {
    solid(color: Color): Background;
    stop(color: Color, percentage: number): BackgroundStop;
    linear_gradient(angle: number, from: Color | BackgroundStop, to: Color | BackgroundStop): Background;
    pattern_slash(color: Color, width: number, interval: number): Background;
    checkerboard(color: Color, size: number): Background;
  };
  export interface DialogOptions {
    escape_dismissable?: boolean;
    backdrop_dismissable?: boolean;
  }
  export interface ToastOptions {
    title: string;
    description?: string;
    level?: "info" | "success" | "warning" | "error";
    /** `null` keeps it up until removed. */
    timeout?: number | null;
    id?: string;
  }
  /** The window the view draws into, as the `window` global. Overlays open from handlers. */
  export interface Window {
    /** Opens a dialog with what `content()` renders, again on every redraw. */
    open_dialog(content: () => Element, options?: DialogOptions): void;
    close_dialog(): void;
    close_all_dialogs(): void;
    has_active_dialog(): boolean;
    /** Opens the sheet on the right. */
    open_sheet(content: () => Element): void;
    open_sheet_at(placement: "top" | "bottom" | "left" | "right", content: () => Element): void;
    close_sheet(): void;
    has_active_sheet(): boolean;
    /** Posts a toast; returns its id. */
    push_toast(options: ToastOptions): string;
    remove_toast(id: string): void;
    clear_toasts(): void;
    /** Dispatches an action down the focus path, as its key binding would. */
    dispatch_action(action: string): void;
    rem_size(): number;
    line_height(): number;
    viewport_size(): Size;
    bounds(): Bounds;
    mouse_position(): Point;
    appearance(): "light" | "dark";
    is_window_active(): boolean;
    is_fullscreen(): boolean;
    is_maximized(): boolean;
    set_rem_size(size: number): void;
    /** Redraws every view of the window; `cx.notify()` is almost always what you want. */
    refresh(): void;
    focus_next(): void;
    focus_prev(): void;
    activate_window(): void;
    minimize_window(): void;
    zoom_window(): void;
    toggle_fullscreen(): void;
    /** An element painting `path` with `fill`, in its own coordinates. */
    paint_path(path: Path, fill: Color | Background): Element;
  }
}
declare var window: import("corona/kit").Window;
declare module "corona/base" {
  import { ClickEvent, Context, Element, FocusHandle } from "corona/kit";
  /** A row, centered on the cross axis. */
  export function h_flex(): Element;
  /** A column. */
  export function v_flex(): Element;
  export type ColorTokens = { readonly [Role in import("corona/kit").ColorToken]: import("corona/kit").Color };
  export type SpacingTokens = { readonly [Step in "xxs" | "xs" | "sm" | "md" | "lg" | "xl" | "xxl"]: number };
  export type RadiusTokens = { readonly [Step in "none" | "sm" | "md" | "lg" | "xl" | "full"]: number };
  export interface TextStyleToken {
    readonly size: number;
    readonly line_height: number;
    /** 100 is thin, 400 regular, 700 bold. */
    readonly weight: number;
  }
  export type TypographyTokens = { readonly sans: string; readonly mono: string } & {
    readonly [Step in "xs" | "sm" | "md" | "lg" | "xl" | "mono_md"]: TextStyleToken;
  };
  export interface SemanticThemeTokens {
    readonly colors: ColorTokens;
    readonly spacing: SpacingTokens;
    readonly radius: RadiusTokens;
    readonly typography: TypographyTokens;
  }
  /** The semantic tokens plus the appearance; the colors also directly. Read-only. */
  export interface Theme extends SemanticThemeTokens, ColorTokens {
    readonly appearance: "light" | "dark";
    readonly is_dark: boolean;
  }
  /** Replaces the tokens it names and redraws every window. Not while rendering. */
  export function set_theme(theme: {
    readonly appearance?: "light" | "dark";
    readonly tokens: {
      readonly colors?: Partial<Record<import("corona/kit").ColorToken, `#${string}`>>;
      readonly spacing?: Partial<SpacingTokens>;
      readonly radius?: Partial<RadiusTokens>;
      readonly typography?: { readonly sans?: string; readonly mono?: string } & {
        readonly [Step in "xs" | "sm" | "md" | "lg" | "xl" | "mono_md"]?: Partial<TextStyleToken>;
      };
    };
  }): void;
  /** Activation, focus, disabled and selected state. No styling. */
  export const Button: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** An external HTTP(S) resource opened through the system browser. */
  export const Link: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A controlled toggle. No styling: draw the indicator yourself. */
  export const Checkbox: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A controlled switch. No styling. */
  export const Switch: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** One option in a radio group. No styling. */
  export const Radio: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A button that stays down: `pressed(..)` in, `on_change(..)` out. */
  export const Toggle: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A set of radios, announced as one group. */
  export const RadioGroup: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A set of toggles, announced as a toolbar. */
  export const ToggleGroup: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A select root: the semantics and the keyboard, none of the picture. */
  export const Select: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A combobox root whose trigger is an editable field. */
  export const Combobox: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A date-picker root; it holds no date. */
  export const DatePicker: { new (id: string | number, focus_handle: FocusHandle): Element; new: (id: string | number, focus_handle: FocusHandle) => Element };
  /** The bare editor of retained text. */
  export const Input: { new (state: InputState): Element; new: (state: InputState) => Element };
  /** A spinbutton; give it `decrement_button`/`increment_button`. */
  export const NumberInput: { new (state: InputState): Element; new: (state: InputState) => Element };
  /** The bare editor of retained multi-line text. */
  export const Textarea: { new (state: TextareaState): Element; new: (state: TextareaState) => Element };
  /** A code drawn cell by cell: `cell_style(..)`, `caret_style(..)`. */
  export const OtpInput: { new (state: OtpState): Element; new: (state: OtpState) => Element };
  /** The slider root: announces the value. */
  export const Slider: { new (state: SliderState): Element; new: (state: SliderState) => Element };
  /** The press and drag surface. */
  export const SliderTrack: { new (state: SliderState): Element; new: (state: SliderState) => Element };
  /** The groove; `range_style(..)` draws the fill. */
  export const SliderIndicator: { new (state: SliderState): Element; new: (state: SliderState) => Element };
  /** The knob; `start(true)` is a range's lower one. */
  export const SliderThumb: { new (state: SliderState): Element; new: (state: SliderState) => Element };
  import { InputState, TextareaState, SliderState, OtpState, SliderValue, TextEvent } from "corona/component";
  export { InputState, TextareaState, SliderState, OtpState, SliderValue, TextEvent };
  type Ctor<A extends any[]> = { new (...args: A): Element; new: (...args: A) => Element };
  /** A tab list; each `Tab` is told whether it is selected and reports clicks. */
  export const Tabs: Ctor<[id: string | number]>;
  /** One tab: `selected(..)` in, `on_click(..)` out. */
  export const Tab: Ctor<[id: string | number]>;
  /** An accordion: items, each with an `AccordionHeader` and an `AccordionPanel`. Draws nothing. */
  export const Accordion: Ctor<[id: string | number]>;
  /** One item: `open(..)`, `header(..)`, `panel(..)`. */
  export const AccordionItem: Ctor<[]>;
  /** The heading owning an item's trigger. */
  export const AccordionHeader: Ctor<[trigger: Element]>;
  /** The region an item reveals; gone while shut unless `keep_mounted()`. */
  export const AccordionPanel: Ctor<[]>;
  /** The button: `on_change(open => ..)` asks for the other state. */
  export const AccordionTrigger: Ctor<[id: string | number]>;
  /** A pagination landmark; draw the pages from `pagination_items`. */
  export const Pagination: Ctor<[id: string | number]>;
  /** One entry of a page layout: a page, or a gap standing for the pages `[first, last]`. */
  export type PaginationEntry =
    | { page: number; ellipsis?: undefined }
    | { ellipsis: [first: number, last: number]; page?: undefined };
  /** Which pages to draw around `current_page`, and where the gaps fall. */
  export function pagination_items(current_page: number, total_pages: number, visible_pages?: number): PaginationEntry[];
  /** Renders its `content` only while `open`; children always. */
  export const Collapsible: Ctor<[]>;
  /** A row of `resizable_panel()`s with draggable dividers; sizes persist under `id`. */
  export function h_resizable(id: string | number): Element;
  /** A column of `resizable_panel()`s. */
  export function v_resizable(id: string | number): Element;
  /** One pane of a resizable group: `size(px)`, `size_range(min, max)`. */
  export function resizable_panel(): Element;
  /** A scroll area's or virtual list's name, or its handle. */
  type ScrollTarget = string | number | import("corona/kit").ScrollHandle | VirtualListScrollHandle;
  /** A scrollbar driving the area that tracks `handle`; it fills the element it is put in. */
  export const Scrollbar: {
    new: (handle: ScrollTarget) => Element;
    horizontal: (handle: ScrollTarget) => Element;
    vertical: (handle: ScrollTarget) => Element;
  };
  /** A list of rows of the given heights that renders only the visible ones. */
  export function v_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element;
  /** `v_virtual_list` along the other axis; `item_sizes` are widths. */
  export function h_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element;
  /** A virtual list's scroll position; `track_scroll(handle)` on the list. */
  export class VirtualListScrollHandle {
    constructor();
    static new(): VirtualListScrollHandle;
    /** Puts item `index` on screen. */
    scroll_to_item(index: number, strategy?: "top" | "center"): void;
    scroll_to_bottom(): void;
  }
  /** An unstyled surface anchored to `trigger(..)`, opened by a press: `content(..)`, `open(..)`, `on_open_change(..)`. */
  export const Popover: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** An unstyled surface anchored to `trigger(..)`, opened by resting the pointer on it. */
  export const HoverCard: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** The bare anchored surface: shows `content(..)` while it is set, anchored to `trigger`. */
  export const Popup: { new (id: string | number, trigger: Element): Element; new: (id: string | number, trigger: Element) => Element };
  /** The progress root: announces `value`, draws nothing; put a `ProgressTrack` in it. */
  export const Progress: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** The groove, a plain element. */
  export const ProgressTrack: { new (): Element; new: () => Element };
  /** The filled part, a plain element: size it from the value. */
  export const ProgressIndicator: { new (): Element; new: () => Element };
  /** Renders its `image` slot, or its `fallback` without one; draws nothing else. */
  export const Avatar: { new (): Element; new: () => Element };
  /** The image slot of an `Avatar`: an asset or an absolute path. */
  export const AvatarImage: { new (source: string): Element; new: (source: string) => Element };
  /** The fallback slot of an `Avatar`. */
  export const AvatarFallback: { new (): Element; new: () => Element };
  /** An unstyled semantic table: nest the parts below. */
  export const Table: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** The header row group. */
  export const TableHeader: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** The body row group. */
  export const TableBody: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** One row, `row_index` one-based. */
  export const TableRow: { new (id: string | number, row_index: number): Element; new: (id: string | number, row_index: number) => Element };
  /** One column header, `column_index` one-based. */
  export const TableHead: { new (id: string | number, column_index: number): Element; new: (id: string | number, column_index: number) => Element };
  /** One data cell, `column_index` one-based. */
  export const TableCell: { new (id: string | number, column_index: number): Element; new: (id: string | number, column_index: number) => Element };
  /** A caption slot. */
  export const TableCaption: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A selected date: a day, a `[start, end]` range, or nothing. Days are `"YYYY-MM-DD"`. */
  export type CalendarDate = string | [string | null, string | null] | null;
  /** A calendar month and its selected date. Retained: create it in `init`. */
  export const CalendarState: { (): CalendarState; new: () => CalendarState };
  export interface CalendarState {
    /** Months of weeks of seven days, the neighbouring months' days included. */
    month_days(): string[][][];
    year(): number;
    /** 1–12. */
    month(): number;
    today(): string;
    value(): CalendarDate;
    /** Selects a day, a range, or nothing, without a `change` event. */
    set_value(next: CalendarDate): void;
    next_month(): void;
    prev_month(): void;
    /** `"change"`: a date was picked in a `Calendar`. */
    on(event: "change", handler: (date: CalendarDate, cx: Context) => void): boolean;
    release(): boolean;
  }
  /** Rich HTML or Markdown text. */
  export const TextView: {
    html(id: string, html: string): Element;
    markdown(id: string, markdown: string): Element;
  };
  /** Where a region sits relative to the center of a dock area. */
  export type DockPlacement = "center" | "left" | "right" | "bottom";
  /** One panel, as `panels()` reports it. */
  export interface DockPanel {
    readonly id: number;
    /** Namespaced: `shell:<plugin>/<name>`. */
    readonly name: string;
    readonly placement: DockPlacement;
    readonly node: number;
    readonly index: number;
    readonly active: boolean;
    readonly visible: boolean;
    readonly closable: boolean;
    readonly zoomable: boolean;
  }
  /** One tab of a group, as a chrome handler gets it. */
  export interface DockTab {
    readonly index: number;
    readonly name: string;
    readonly id: number;
    readonly active: boolean;
    readonly visible: boolean;
    readonly closable: boolean;
    readonly zoomable: boolean;
  }
  /** A tab group, as `tab_bar` and `empty_group` get it. */
  export interface DockGroup {
    readonly area: number;
    readonly node: number;
    readonly active_index: number;
    readonly zoomed: boolean;
    readonly collapsed: boolean;
    readonly locked: boolean;
    readonly draggable: boolean;
    readonly droppable: boolean;
    readonly closable: boolean;
    readonly tabs: readonly DockTab[];
  }
  /** One dock, as the `dock` handler gets it. */
  export interface DockRegion {
    readonly area: number;
    readonly placement: DockPlacement;
    readonly size: number;
    readonly open: boolean;
    readonly collapsible: boolean;
  }
  export interface DockRect { readonly x: number; readonly y: number; readonly width: number; readonly height: number }
  /** Where a dragged panel would land; `placement` null merges into the group's tabs. */
  export interface DockDrop {
    readonly placement: "left" | "right" | "top" | "bottom" | null;
    readonly bounds: DockRect;
    readonly from: DockRect;
    readonly to: DockRect;
  }
  export interface DockPanelOptions {
    /** What a saved layout files it under, and `register_panel` rebuilds it by. */
    name: string;
    placement?: DockPlacement;
    size?: number;
    closable?: boolean;
    zoomable?: boolean;
    visible?: boolean;
  }
  /** A dockable layout the user rearranges. Make it in `init` or a handler. */
  export interface DockArea {
    /** Docks `view`, a view from `cx.new(Class)`. */
    add_panel(view: import("corona/kit").Entity, options: DockPanelOptions): void;
    remove_panel(id: number): boolean;
    panels(): DockPanel[];
    /** The layout as plain data, with each panel's `serialize()`. */
    dump(): any;
    /** Restores a `dump()`, rebuilding registered panels and calling their `deserialize(data)`. */
    load(state: any): void;
    has_dock(placement: DockPlacement): boolean;
    is_dock_open(placement: DockPlacement): boolean;
    toggle_dock(placement: DockPlacement): void;
    remove_dock(placement: DockPlacement): void;
    dock_size(placement: DockPlacement): number | null;
    set_dock_size(placement: DockPlacement, size: number): void;
    set_dock_collapsible(placement: DockPlacement, collapsible: boolean): void;
    is_locked(): boolean;
    set_locked(locked: boolean): void;
    is_zoomed(): boolean;
    zoom_out(): void;
    on(event: "layout_changed", handler: (cx: Context) => void): boolean;
    release(): boolean;
  }
  export const DockArea: {
    new: (id: string, options?: { version?: number }) => DockArea;
    /** Rebuilds `name`'s panel from `Class` when a saved layout names it; answers the qualified name. */
    register_panel: (name: string, Class: import("corona/kit").ViewClass) => string;
  };
  /** Draws a dock area; its chrome comes from `tab_bar`, `empty_group`, `drop_indicator`, `dock`. */
  export function dock_area(area: DockArea): Element;
  /** Where a dock's panels go inside its chrome handler. */
  export const dock_content: { new (): Element; new: () => Element };
}
declare module "corona/component" {
  import { ClickEvent, Context, Element, FocusHandle } from "corona/kit";
  export const Probe: { new (): Element; new: () => Element };
  /** A styled command button. */
  export const Button: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** An external-resource link. */
  export const Link: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A controlled check box: `checked(..)` in, `on_change(..)` out. */
  export const Checkbox: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A controlled switch: `checked(..)` in, `on_change(..)` out. */
  export const Switch: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** One option of a radio group. */
  export const Radio: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A column of `Radio`s; `on_change(index => ..)`. */
  export const RadioGroup: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A row of `Radio`s; `on_change(index => ..)`. */
  export const HorizontalRadioGroup: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A button that stays down: `checked(..)` in, `on_change(..)` out. */
  export const Toggle: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A button copying `value(..)`. */
  export const Clipboard: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** Stars: `value(..)`, `max(..)`, `on_change(stars => ..)`. */
  export const Rating: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A frame around `input(..)` and its `addon(..)`s. */
  export const InputGroup: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** An addon of an `InputGroup`, at `align(..)`. */
  export const InputGroupAddon: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A compact button inside an `InputGroup`. */
  export const InputGroupButton: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** The frame around retained text. */
  export const Input: { new (state: InputState): Element; new: (state: InputState) => Element };
  /** A spinbutton over an `InputState`. */
  export const NumberInput: { new (state: InputState): Element; new: (state: InputState) => Element };
  /** The single-line control of an `InputGroup`. */
  export const InputGroupInput: { new (state: InputState): Element; new: (state: InputState) => Element };
  /** The frame around retained multi-line text. */
  export const Textarea: { new (state: TextareaState): Element; new: (state: TextareaState) => Element };
  /** The multi-line control of an `InputGroup`. */
  export const InputGroupTextarea: { new (state: TextareaState): Element; new: (state: TextareaState) => Element };
  /** A fixed-length code. */
  export const OtpInput: { new (state: OtpState): Element; new: (state: OtpState) => Element };
  /** A slider over a retained `SliderState`. */
  export const Slider: { new (state: SliderState): Element; new: (state: SliderState) => Element };
  /** A color picker. */
  export const ColorPicker: { new (state: ColorPickerState): Element; new: (state: ColorPickerState) => Element };
  /** A date picker. */
  export const DatePicker: { new (state: DatePickerState): Element; new: (state: DatePickerState) => Element };
  /** A time of day field. */
  export const TimeField: { new (state: TimeFieldState): Element; new: (state: TimeFieldState) => Element };
  /** A text label. */
  export const Label: { new (text: string): Element; new: (text: string) => Element };
  /** A keyboard shortcut keycap, e.g. `"ctrl-s"`. */
  export const Kbd: { new (keystroke: string): Element; new: (keystroke: string) => Element };
  /** A labeled form field. */
  export const Field: { new (): Element; new: () => Element };
  /** A form of `Field`s. */
  export const Form: { new (): Element; new: () => Element };
  /** A form laying out its labels above. */
  export const VForm: { new (): Element; new: () => Element };
  /** A form laying out its labels beside. */
  export const HForm: { new (): Element; new: () => Element };
  /** Muted text inside an `InputGroup`. */
  export const InputGroupText: { new (): Element; new: () => Element };
  /** A native select over the `{ id, label, disabled? }` rows `rows()` returns. */
  export const Select: { new (id: string, rows: () => readonly SelectRow[], render_row: (row: any) => Element | null, on_select: (value: string, cx: Context) => void): Element; new: (id: string, rows: () => readonly SelectRow[], render_row: (row: any) => Element | null, on_select: (value: string, cx: Context) => void) => Element };
  /** A single-select searchable combobox over the rows `rows()` returns. */
  export const Combobox: { new (id: string, rows: () => readonly SelectRow[], on_change: (value: string, cx: Context) => void, on_confirm: (value: string, cx: Context) => void): Element; new: (id: string, rows: () => readonly SelectRow[], on_change: (value: string, cx: Context) => void, on_confirm: (value: string, cx: Context) => void) => Element };
  /** Half-open UTF-16 string offsets, as `slice()` takes them. */
  export interface InputRange { start: number; end: number }
  export interface InlineToken { id: string; text: string; label?: string }
  export interface InlineTokenSpan { range: InputRange; token: InlineToken }
  export interface InputContent { text: string; tokens: InlineTokenSpan[] }
  export interface InlineTokenContext extends InlineTokenSpan {
    selected: boolean; disabled: boolean; readonly: boolean;
    line_height: number; available_width: number;
  }
  export interface InlineTokenClickEvent extends InlineTokenSpan {
    bounds: { x: number; y: number; width: number; height: number };
    modifiers: { shift: boolean; alt: boolean; control: boolean; platform: boolean };
  }
  export interface InlineTokenHoverEvent extends InlineTokenSpan {
    hovered: boolean;
    bounds: { x: number; y: number; width: number; height: number };
  }
  /** A row of a `Select` or `Combobox`. */
  export interface SelectRow { id: string; label: string; disabled?: boolean; }
  /** What a text state's `on` handlers get; `submit` carries the keys held with Enter. */
  export interface TextEvent { secondary?: boolean; shift?: boolean; }
  /** Retained text of an `Input` or `NumberInput`. Make it in `init` or a handler. */
  export interface InputState {
    value(): string;
    /** Replaces the text, or the text and its tokens; emits no `change`. */
    set_value(value: string | InputContent): void;
    /** The text with its tokens, ranges in UTF-16 offsets as `slice()` takes them. */
    content(): InputContent;
    tokens(): InlineTokenSpan[];
    /** Replaces the selection with an atomic token. */
    replace_with_token(token: InlineToken): void;
    replace_range_with_token(range: InputRange, token: InlineToken): void;
    set_selected_range(range: InputRange): void;
    /** Replaces the selection with `text`, as typing it would. */
    replace(text: string): void;
    set_placeholder(text: string): void;
    set_masked(masked: boolean): void;
    set_loading(loading: boolean): void;
    /** How far a `NumberInput` steps; `null` stops stepping. */
    set_step(step: number | null): void;
    set_min(min: number | null): void;
    set_max(max: number | null): void;
    focus(): void;
    on(event: "change" | "submit" | "focus" | "blur", handler: (event: TextEvent, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const InputState: {
    (placeholder?: string, initial_value?: string): InputState;
    new: (options?: { placeholder?: string; value?: string }) => InputState;
  };
  /** Retained text of a `Textarea`. */
  export interface TextareaState {
    value(): string;
    set_value(value: string | InputContent): void;
    /** The text with its tokens, ranges in UTF-16 offsets as `slice()` takes them. */
    content(): InputContent;
    tokens(): InlineTokenSpan[];
    /** Replaces the selection with an atomic token. */
    replace_with_token(token: InlineToken): void;
    replace_range_with_token(range: InputRange, token: InlineToken): void;
    set_selected_range(range: InputRange): void;

    replace(text: string): void;
    set_placeholder(text: string): void;
    set_rows(rows: number): void;
    set_auto_grow(min_rows: number, max_rows: number): void;
    set_soft_wrap(wrap: boolean): void;
    focus(): void;
    on(event: "change" | "submit" | "focus" | "blur", handler: (event: TextEvent, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const TextareaState: {
    (initial_value?: string): TextareaState;
    new: (options?: { placeholder?: string; value?: string; rows?: number }) => TextareaState;
  };
  /** One thumb, or the two ends of a range. */
  export type SliderValue = number | [number, number];
  /** Where a `Slider` is; a drag writes it without rendering the script. */
  export interface SliderState {
    value(): SliderValue;
    set_value(value: SliderValue): void;
    min_value(): number;
    max_value(): number;
    step_value(): number;
    /** `change` on every step of a drag, `release` once when it ends. */
    on(event: "change" | "release", handler: (value: SliderValue, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const SliderState: {
    (initial_value?: number): SliderState;
    new: (options?: { min?: number; max?: number; step?: number; scale?: "linear" | "logarithmic"; value?: SliderValue }) => SliderState;
  };
  /** The code an `OtpInput` edits; its length is fixed. */
  export interface OtpState {
    value(): string;
    set_value(value: string): void;
    len(): number;
    is_masked(): boolean;
    set_masked(masked: boolean): void;
    focus(): void;
    on(event: "change" | "complete" | "focus" | "blur", handler: (event: {}, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const OtpState: {
    (length: number, options?: { value?: string; masked?: boolean }): OtpState;
    new: (length: number, options?: { value?: string; masked?: boolean }) => OtpState;
  };
  export interface ColorPickerState {
    /** The color as `#rrggbb[aa]`. */
    value(): string | undefined;
    set_value(color: import("corona/kit").Color | null): void;
    on(event: "change", handler: (color: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function ColorPickerState(): ColorPickerState;
  export interface DatePickerState {
    /** The picked date as `YYYY-MM-DD`. */
    value(): string | undefined;
    on(event: "change", handler: (date: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function DatePickerState(): DatePickerState;
  export interface TimeFieldState {
    /** The time as `HH:MM:SS`. */
    value(): string;
    on(event: "change", handler: (time: string, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function TimeFieldState(): TimeFieldState;
  type Ctor<A extends any[]> = { new (...args: A): Element; new: (...args: A) => Element };
  /** A styled tab bar taking `Tab` children. */
  export const TabBar: Ctor<[id: string | number]>;
  /** @deprecated Use `TabBar`. */
  export const Tabs: Ctor<[id: string | number]>;
  /** One tab of a `TabBar`. */
  export const Tab: Ctor<[]>;
  /** An accordion taking `AccordionItem` children. */
  export const Accordion: Ctor<[id: string | number]>;
  /** One item: `title(..)`, `open(..)`, children as its content. */
  export const AccordionItem: Ctor<[]>;
  /** Renders its `content` only while `open`. */
  export const Collapsible: Ctor<[]>;
  /** Page buttons: `current_page`, `total_pages`, `on_change(page => ..)`. */
  export const Pagination: Ctor<[id: string | number]>;
  /** Steps taking `StepperItem` children. */
  export const Stepper: Ctor<[id: string | number]>;
  export const StepperItem: Ctor<[]>;
  /** A trail of labels. */
  export const Breadcrumb: Ctor<[labels: string[]]>;
  /** A titled container. */
  export const GroupBox: Ctor<[]>;
  /** A bar: children in the middle, `left_content`/`right_content` at the edges. */
  export const StatusBar: Ctor<[]>;
  export const Toolbar: Ctor<[id: string | number]>;
  export const Separator: Ctor<[]>;
  export const VerticalSeparator: Ctor<[]>;
  export const DashedSeparator: Ctor<[]>;
  export const VerticalDashedSeparator: Ctor<[]>;
  /** Panes with draggable dividers, horizontal unless `axis("vertical")`; takes `ResizablePanel`s. */
  export const Resizable: Ctor<[id: string | number]>;
  export const ResizablePanel: Ctor<[]>;
  /** A vertically scrolling area tracking `handle`; `scroll_axis(..)` changes the axes. */
  export const Scroll: Ctor<[handle: ScrollbarHandle]>;
  /** A scrollbar driving the area that tracks `handle`. */
  export const Scrollbar: Ctor<[id: string | number, handle: string | number | ScrollbarHandle | import("corona/base").VirtualListScrollHandle]>;
  export type ScrollbarHandle = import("corona/kit").ScrollHandle;
  export const ScrollbarHandle: { (): ScrollbarHandle; new: () => ScrollbarHandle };
  /** A collapsible sidebar taking `SidebarMenu` children, `header(..)` and `footer(..)`. */
  export const Sidebar: Ctor<[id: string | number]>;
  export const SidebarHeader: Ctor<[]>;
  export const SidebarFooter: Ctor<[]>;
  /** Rows: `SidebarMenuItem` children. */
  export const SidebarMenu: Ctor<[]>;
  /** A row; `SidebarMenuItem` children make its submenu. */
  export const SidebarMenuItem: Ctor<[label: string]>;
  export const SidebarToggleButton: Ctor<[]>;
  /** A button opening a menu of `item(label, cx => ..)`s. */
  export const DropdownMenu: Ctor<[id: string | number, label: string]>;
  /** A button popping up the OS menu of its `NativeMenuItem` children. */
  export const NativeMenuTrigger: Ctor<[id: string | number, label: string]>;
  /** An item dispatching `action`, for `on_action`. */
  export const NativeMenuItem: Ctor<[label: string, action: string]>;
  export const NativeMenuSeparator: Ctor<[]>;
  /** A command palette's query and selection; make it in `init`. */
  export interface CommandState {
    /** The search text. */
    query(): string;
    release(): boolean;
  }
  export const CommandState: { (): CommandState; new: () => CommandState };
  /** A searchable command palette taking `CommandItem`, `CommandGroup` and `CommandSeparator`. */
  export const Command: Ctor<[state: CommandState]>;
  export const CommandItem: Ctor<[label: string]>;
  /** A labelled section taking `CommandItem` children. */
  export const CommandGroup: Ctor<[label: string]>;
  export const CommandSeparator: Ctor<[]>;
  /** An application menu of `MenuItem`s and `MenuSeparator`s, for a `MenuBar`. */
  export const Menu: Ctor<[label: string]>;
  /** A menu item dispatching `action`, for `on_action`. */
  export const MenuItem: Ctor<[label: string, action: string]>;
  export const MenuSeparator: Ctor<[]>;
  /** Installs its `Menu` children as the application menus and draws them in the window. */
  export const MenuBar: Ctor<[id: string]>;
  /** A labelled button with a menu of `menu_item(label, cx => ..)`s. */
  export const DropdownButton: Ctor<[id: string | number, label: string]>;
  /** A ghost button labelled `label` opening a styled surface: `content(..)`, `open(..)`, `on_open_change(..)`. */
  export const Popover: { new (id: string, label: string): Element; new: (id: string, label: string) => Element };
  /** A styled surface opened by hovering `trigger_element(..)`. */
  export const HoverCard: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A button `label` showing `text` on hover. */
  export const Tooltip: { new (id: string, label: string, text: string): Element; new: (id: string, label: string, text: string) => Element };
  /** A button `label` opening a dialog of `title(..)` and `content(() => ..)`. */
  export const Dialog: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element };
  /** A button `label` opening an alert of `title(..)` and `description(..)`. */
  export const AlertDialog: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element };
  /** A button `label` opening a sheet of `content(() => ..)` at `placement(..)`. */
  export const Sheet: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element };
  /** A button `label` opening a toast of `title(..)` and `message(..)`. */
  export const Notification: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element };
  /** An inline message. */
  export const Alert: { new (id: string | number, message: string): Element; new: (id: string | number, message: string) => Element };
  /** An inline message. */
  export const InfoAlert: { new (id: string | number, message: string): Element; new: (id: string | number, message: string) => Element };
  /** An inline message. */
  export const SuccessAlert: { new (id: string | number, message: string): Element; new: (id: string | number, message: string) => Element };
  /** An inline message. */
  export const WarningAlert: { new (id: string | number, message: string): Element; new: (id: string | number, message: string) => Element };
  /** An inline message. */
  export const ErrorAlert: { new (id: string | number, message: string): Element; new: (id: string | number, message: string) => Element };
  import { CalendarState, CalendarDate } from "corona/base";
  export { CalendarState, CalendarDate };
  export interface ChartRow {
    label: string;
    value: number;
  }
  export type ChartRows = readonly ChartRow[] | (() => readonly ChartRow[]);
  /** Which slide a `Carousel` shows. Retained: create it in `init`. */
  export const CarouselState: {
    (item_count: number, selected_index?: number, axis?: "horizontal" | "vertical", looping?: boolean): CarouselState;
    new: (item_count: number, selected_index?: number, axis?: "horizontal" | "vertical", looping?: boolean) => CarouselState;
  };
  export interface CarouselState {
    item_count(): number;
    selected_index(): number | undefined;
    /** Selects a slide without a `change` event. */
    set_selected_index(index: number): void;
    set_item_count(count: number): void;
    /** Moves on, with a `change` event; false at the end. */
    select_next(): boolean;
    select_previous(): boolean;
    /** `"change"`: another slide was selected. */
    on(event: "change", handler: (index: number, cx: Context) => void): boolean;
    release(): boolean;
  }
  /** The columns, selection and scroll of a `DataTable`. Retained: create it in `init`. */
  export const DataTableState: { (columns: string[]): DataTableState; new: (columns: string[]) => DataTableState };
  export interface DataTableState {
    on(event: "select_row" | "double_click_row" | "select_column", handler: (index: number, cx: Context) => void): boolean;
    release(): boolean;
  }
  /** A count or dot over its children. */
  export const Badge: { new (): Element; new: () => Element };
  /** A small label. */
  export const Tag: { new (): Element; new: () => Element };
  /** A loading placeholder; size it. */
  export const Skeleton: { new (): Element; new: () => Element };
  /** A turning loading icon. */
  export const Spinner: { new (): Element; new: () => Element };
  /** An empty state: `header`, `content` and children. */
  export const Empty: { new (): Element; new: () => Element };
  /** The header of an `Empty`: `media`, `title`, `description`. */
  export const EmptyHeader: { new (): Element; new: () => Element };
  /** The media of an `EmptyHeader`. */
  export const EmptyMedia: { new (): Element; new: () => Element };
  /** The title of an `EmptyHeader`. */
  export const EmptyTitle: { new (): Element; new: () => Element };
  /** The description of an `EmptyHeader`. */
  export const EmptyDescription: { new (): Element; new: () => Element };
  /** The actions of an `Empty`. */
  export const EmptyContent: { new (): Element; new: () => Element };
  /** An icon from the app's assets. */
  export const Icon: { new (path: string): Element; new: (path: string) => Element };
  /** An image from the app's assets, or a file by absolute path. */
  export const Image: { new (path: string): Element; new: (path: string) => Element };
  /** Labelled values: add `DescriptionItem`s with `items`. */
  export const DescriptionList: { new (): Element; new: () => Element };
  /** One labelled value of a `DescriptionList`. */
  export const DescriptionItem: { new (label: string): Element; new: (label: string) => Element };
  /** A progress bar, `value` 0–100. */
  export const Progress: { new (id: string | number): Element; new: (id: string | number) => Element };
  /** A round picture, or the initials of `name`. */
  export const Avatar: { new (): Element; new: () => Element };
  /** Bars of `{ label, value }` rows, or a function returning them; fills its size. */
  export const BarChart: { new (rows: ChartRows): Element; new: (rows: ChartRows) => Element };
  /** A line through the rows. */
  export const LineChart: { new (rows: ChartRows): Element; new: (rows: ChartRows) => Element };
  /** A filled line through the rows. */
  export const AreaChart: { new (rows: ChartRows): Element; new: (rows: ChartRows) => Element };
  /** Slices of the rows. */
  export const PieChart: { new (rows: ChartRows): Element; new: (rows: ChartRows) => Element };
  /** A radar of the rows. */
  export const RadarChart: { new (rows: ChartRows): Element; new: (rows: ChartRows) => Element };
  /** A month grid of a `CalendarState`, picking dates. */
  export const Calendar: { new (state: CalendarState): Element; new: (state: CalendarState) => Element };
  /** Slides: `CarouselContent` with `CarouselItem`s, and the controls. */
  export const Carousel: { new (id: string | number, state: CarouselState): Element; new: (id: string | number, state: CarouselState) => Element };
  /** The clipped viewport of a `Carousel`. */
  export const CarouselContent: { new (state: CarouselState): Element; new: (state: CarouselState) => Element };
  /** Slide `index`. */
  export const CarouselItem: { new (id: string | number, index: number, state: CarouselState): Element; new: (id: string | number, index: number, state: CarouselState) => Element };
  /** The previous-slide button. */
  export const CarouselPrevious: { new (state: CarouselState): Element; new: (state: CarouselState) => Element };
  /** The next-slide button. */
  export const CarouselNext: { new (state: CarouselState): Element; new: (state: CarouselState) => Element };
  /** The row of `CarouselPaginationItem`s. */
  export const CarouselPagination: { new (): Element; new: () => Element };
  /** The dot of slide `index`. */
  export const CarouselPaginationItem: { new (id: string | number, index: number, state: CarouselState): Element; new: (id: string | number, index: number, state: CarouselState) => Element };
  /** A list of rows rendered lazily by `render_row`; its state is kept by `id`. */
  export const List: { new (id: string, rows: readonly unknown[] | (() => readonly unknown[]), render_row: (row: any, cx: Context) => Element): Element; new: (id: string, rows: readonly unknown[] | (() => readonly unknown[]), render_row: (row: any, cx: Context) => Element) => Element };
  /** A table of rows, its visible cells rendered lazily by `render_cell`. */
  export const DataTable: { new (state: DataTableState, rows: readonly unknown[] | (() => readonly unknown[]), render_cell: (row: any, column: string, cx: Context) => Element): Element; new: (state: DataTableState, rows: readonly unknown[] | (() => readonly unknown[]), render_cell: (row: any, column: string, cx: Context) => Element) => Element };
  /** A tree of `TreeItem` children; what is expanded is kept by `id`. */
  export const Tree: { new (id: string): Element; new: (id: string) => Element };
  /** One node of a `Tree`; nest more as children. */
  export const TreeItem: { new (id: string, label: string): Element; new: (id: string, label: string) => Element };
  /** A styled table of `TableHeader`, `TableBody`, `TableFooter` and `TableCaption`. */
  export const Table: { new (): Element; new: () => Element };
  /** The header rows of a `Table`. */
  export const TableHeader: { new (): Element; new: () => Element };
  /** The body rows of a `Table`. */
  export const TableBody: { new (): Element; new: () => Element };
  /** The footer rows of a `Table`. */
  export const TableFooter: { new (): Element; new: () => Element };
  /** A row of `TableHead`s or `TableCell`s. */
  export const TableRow: { new (): Element; new: () => Element };
  /** A column header cell. */
  export const TableHead: { new (): Element; new: () => Element };
  /** A data cell. */
  export const TableCell: { new (): Element; new: () => Element };
  /** A caption under a `Table`. */
  export const TableCaption: { new (): Element; new: () => Element };
  export type ComponentSize = "xsmall" | "small" | "medium" | "large";
  /** A file or image attachment; its children fill its content. */
  export const Attachment: { new (id: string): Element; new: (id: string) => Element };
  /** A message bubble; its children are its content. */
  export const Bubble: { new (): Element; new: () => Element };
  /** A compact conversation status marker. */
  export const Marker: { new (id: string): Element; new: (id: string) => Element };
  /** A message row; its children fill its content. */
  export const Message: { new (): Element; new: () => Element };
  /** Animated loading text. */
  export const ShimmerText: { new (text: string): Element; new: (text: string) => Element };
  /** Retained virtual-list and tail-following state for a message transcript. */
  export interface MessageScrollerState {
    item_count(): number;
    /** Replaces every row with `count` new ones. */
    reset(count: number): void;
    /** Adds `count` rows at the end, following them when at the tail. */
    append(count: number): boolean;
    scroll_to_end(): void;
    is_following_tail(): boolean;
    release(): boolean;
  }
  export const MessageScrollerState: {
    (item_count: number): MessageScrollerState;
    new (item_count: number): MessageScrollerState;
    new: (item_count: number) => MessageScrollerState;
  };
  /** A virtualized message transcript; `render_item(index)` renders a row. */
  export const MessageScroller: {
    new (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null): Element;
    new: (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null) => Element;
  };
  /** Retained source-editor state. Made in `init` or a handler: it needs the window. */
  export interface EditorState {
    value(): string;
    /** Replaces the text, without a `change` event. */
    set_value(text: string): void;
    /** `"change"` and `"enter"` pass the text, `"focus"` and `"blur"` nothing; false for another name. */
    on(event: "change" | "enter" | "focus" | "blur", handler: (value: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const EditorState: {
    (initial_value: string, language?: "rust" | "json"): EditorState;
    new (initial_value: string, language?: "rust" | "json"): EditorState;
    new: (initial_value: string, language?: "rust" | "json") => EditorState;
  };
  /** A source editor; takes no children. */
  export const Editor: { new (state: EditorState): Element; new: (state: EditorState) => Element };
  /** Retained readonly patch state, from unified or Git diff text. */
  export interface DiffState {
    /** Shows another patch. */
    set_patch(patch: string): void;
    release(): boolean;
  }
  export const DiffState: {
    (patch: string, mode?: "unified" | "split"): DiffState;
    new (patch: string, mode?: "unified" | "split"): DiffState;
    new: (patch: string, mode?: "unified" | "split") => DiffState;
  };
  /** A readonly patch viewer; takes no children. */
  export const Diff: { new (state: DiffState): Element; new: (state: DiffState) => Element };
  /** Plain text in a styleable wrapper. */
  export const Text: { new (value: string): Element; new: (value: string) => Element };
  /** A settings surface; takes `SettingPage` children. */
  export const Settings: { new (id: string): Element; new: (id: string) => Element };
  /** A settings page; takes `SettingGroup` children. */
  export const SettingPage: { new (title: string): Element; new: (title: string) => Element };
  /** A settings group; takes `SettingItem` children. */
  export const SettingGroup: { new (): Element; new: () => Element };
  /** One setting; `content(() => field)` renders its field. */
  export const SettingItem: { new (title: string): Element; new: (title: string) => Element };
  /** A questionnaire keyed by `id`; takes `QuestionnaireItem` children, rebuilt when they change. */
  export const Questionnaire: { new (id: string): Element; new: (id: string) => Element };
  /** One question; takes `QuestionnaireChoice` children. */
  export const QuestionnaireItem: { new (name: string, label: string): Element; new: (name: string, label: string) => Element };
  /** A free-form answer of a question, typed into `state`. */
  export const QuestionnaireInput: { new (state: InputState, label: string): Element; new: (state: InputState, label: string) => Element };
  /** One answer of a question. */
  export const QuestionnaireChoice: { new (value: string, label: string): Element; new: (value: string, label: string) => Element };
}
declare module "corona/fps" {
  import { Element } from "corona/kit";
  export type Anchor = "top_left" | "top_right" | "bottom_left" | "bottom_right" | "top_center" | "bottom_center" | "left_center" | "right_center";
  /** The performance HUD as an element; its parent must be `relative()`. */
  export function fps_monitor(): Element;
  export interface FpsMonitorOptions {
    /** Default `top_right`. */
    anchor?: Anchor;
    /** Frame budget in milliseconds. */
    frame_budget?: number;
  }
  /** Draws the HUD over the window until `hide_fps_monitor()`; again, it moves it. */
  export function show_fps_monitor(options?: FpsMonitorOptions): void;
  /** Takes the HUD down; true if it was up. */
  export function hide_fps_monitor(): boolean;
  export function fps_monitor_visible(): boolean;
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
