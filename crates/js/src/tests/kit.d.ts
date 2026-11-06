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
  export type NullaryStyles<Self> = { [Method in NullaryStyle]: (this: Element<StyledKind>) => Self };
  /** The kinds of element taking style methods. */
  export type StyledKind = "Probe" | "Accordion" | "Accordion@base" | "AccordionHeader@base" | "AccordionItem" | "AccordionItem@base" | "AccordionPanel@base" | "AccordionTrigger@base" | "Alert" | "AlertDialog" | "AreaChart" | "Attachment" | "Avatar" | "Avatar@base" | "AvatarFallback@base" | "AvatarImage" | "AvatarImage@base" | "Badge" | "BarChart" | "Breadcrumb" | "Bubble" | "Button" | "Button@base" | "Calendar" | "Carousel" | "CarouselContent" | "CarouselItem" | "CarouselNext" | "CarouselPagination" | "CarouselPaginationItem" | "CarouselPrevious" | "Checkbox" | "Checkbox@base" | "Collapsible" | "Collapsible@base" | "ColorPicker" | "Combobox" | "Combobox@base" | "Command" | "DataTable" | "DatePicker" | "DatePicker@base" | "DescriptionList" | "Dialog" | "Diff" | "DropdownButton" | "DropdownMenu" | "Editor" | "Empty" | "EmptyContent" | "EmptyDescription" | "EmptyHeader" | "EmptyMedia" | "EmptyTitle" | "Field" | "Form" | "GroupBox" | "HoverCard" | "Icon" | "Input" | "InputGroup" | "InputGroupAddon" | "InputGroupButton" | "InputGroupText" | "Kbd" | "Label" | "LineChart" | "Link" | "Link@base" | "List" | "Marker" | "Message" | "MessageScroller" | "NativeMenuTrigger" | "Notification" | "NumberInput" | "NumberInput@base" | "OtpInput@base" | "Pagination" | "Pagination@base" | "PieChart" | "Popover" | "Popover@base" | "Popup@base" | "Probe" | "Progress" | "Progress@base" | "ProgressIndicator@base" | "ProgressTrack@base" | "Questionnaire" | "RadarChart" | "Radio" | "Radio@base" | "RadioGroup" | "RadioGroup@base" | "Rating" | "Resizable" | "Resizable@base" | "ResizablePanel" | "ResizablePanel@base" | "Scrollable" | "Scroll" | "Select" | "Select@base" | "Separator" | "SettingGroup" | "Sheet" | "ShimmerText" | "Sidebar" | "SidebarFooter" | "SidebarHeader" | "Skeleton" | "Slider" | "Slider@base" | "SliderIndicator@base" | "SliderThumb@base" | "SliderTrack@base" | "Spinner" | "StatusBar" | "Stepper" | "StepperItem" | "Switch" | "Switch@base" | "Tab" | "Tab@base" | "TabBar" | "Table" | "Table@base" | "TableBody" | "TableBody@base" | "TableCaption" | "TableCaption@base" | "TableCell" | "TableCell@base" | "TableFooter" | "TableHead" | "TableHead@base" | "TableHeader" | "TableHeader@base" | "TableRow" | "TableRow@base" | "Tabs@base" | "Tag" | "TextView@base" | "Textarea" | "TimeField" | "Toggle" | "Toggle@base" | "ToggleGroup@base" | "Toolbar" | "Tree" | "VirtualList@base" | "canvas" | "div" | "dock_area@base" | "dock_content" | "dock_content@base" | "image" | "list" | "style" | "svg" | "uniform_list";
  /** The kinds of element taking `id`, `on_click`, .. */
  export type InteractiveKind = "Accordion@base" | "AccordionTrigger@base" | "AreaChart" | "Avatar" | "Avatar@base" | "AvatarImage" | "AvatarImage@base" | "Badge" | "BarChart" | "Button" | "Button@base" | "Checkbox" | "Checkbox@base" | "DatePicker@base" | "DescriptionList" | "InputGroupButton" | "Label" | "LineChart" | "Link@base" | "Pagination@base" | "PieChart" | "Popup@base" | "Progress@base" | "RadarChart" | "Radio" | "Radio@base" | "RadioGroup@base" | "Scrollable" | "Scroll" | "SidebarFooter" | "SidebarHeader" | "SliderTrack@base" | "Spinner" | "Switch@base" | "Tab" | "Tab@base" | "Table@base" | "TableBody@base" | "TableCaption@base" | "TableCell@base" | "TableHead@base" | "TableHeader@base" | "TableRow@base" | "Tabs@base" | "Toggle@base" | "ToggleGroup@base" | "div" | "image" | "svg" | "uniform_list";
  /** The kinds of element taking `child`. */
  export type ParentKind = "Accordion" | "Accordion@base" | "AccordionHeader@base" | "AccordionItem" | "AccordionItem@base" | "AccordionPanel@base" | "AccordionTrigger@base" | "Attachment" | "AvatarFallback@base" | "Badge" | "Bubble" | "Button" | "Button@base" | "Carousel" | "CarouselContent" | "CarouselItem" | "CarouselNext" | "CarouselPagination" | "CarouselPaginationItem" | "CarouselPrevious" | "Checkbox" | "Checkbox@base" | "Collapsible" | "Collapsible@base" | "Combobox@base" | "Command" | "CommandGroup" | "DatePicker@base" | "DescriptionList" | "Empty" | "EmptyContent" | "EmptyDescription" | "EmptyMedia" | "EmptyTitle" | "Field" | "Form" | "GroupBox" | "HoverCard" | "InputGroupAddon" | "InputGroupButton" | "InputGroupText" | "Link" | "Link@base" | "Marker" | "Menu" | "MenuBar" | "Message" | "NativeMenuTrigger" | "NumberInput@base" | "OtpInput@base" | "Pagination@base" | "Popover" | "Progress@base" | "ProgressIndicator@base" | "ProgressTrack@base" | "Questionnaire" | "QuestionnaireItem" | "Radio" | "Radio@base" | "RadioGroup" | "RadioGroup@base" | "Resizable" | "Resizable@base" | "ResizablePanel" | "ResizablePanel@base" | "Scrollable" | "Scroll" | "Select@base" | "SettingGroup" | "SettingPage" | "Settings" | "Sidebar" | "SidebarFooter" | "SidebarHeader" | "SidebarMenu" | "SidebarMenuItem" | "Slider@base" | "SliderIndicator@base" | "SliderThumb@base" | "SliderTrack@base" | "StatusBar" | "Stepper" | "StepperItem" | "Switch@base" | "Tab" | "Tab@base" | "TabBar" | "Table" | "Table@base" | "TableBody" | "TableBody@base" | "TableCaption" | "TableCaption@base" | "TableCell" | "TableCell@base" | "TableFooter" | "TableHead" | "TableHead@base" | "TableHeader" | "TableHeader@base" | "TableRow" | "TableRow@base" | "Tabs@base" | "Tag" | "Toggle" | "Toggle@base" | "ToggleGroup@base" | "Toolbar" | "Tree" | "TreeItem" | "div";
  /** What an element takes: style methods, interaction methods, `child`. */
  export type Capability = "style" | "interactive" | "parent";
  /**
   * An element of any kind taking all of `C`, e.g. `ElementOf<"style" | "parent">` for a helper
   * returning a `div()`, an `h_flex()` or a `Button`.
   */
  export type ElementOf<C extends Capability> = Element<
    ("style" extends C ? StyledKind : string) &
      ("interactive" extends C ? InteractiveKind : string) &
      ("parent" extends C ? ParentKind : string)
  >;

  /** An element of the render that built it: chain its methods, pass it to `child`. `K` is its kind: a component's methods are typed on the kinds having them. */
  export interface Element<K extends string = string> extends NullaryStyles<Element<K>> {
    readonly __kind?: K;
    /** Sets the width. */
    w(this: Element<StyledKind>, v: Length): this;
    /** Sets the height. */
    h(this: Element<StyledKind>, v: Length): this;
    /** Sets the width and the height together. */
    size(this: Element<StyledKind>, v: Length): this;
    /** Sets the minimum width. */
    min_w(this: Element<StyledKind>, v: Length): this;
    /** Sets the minimum height. */
    min_h(this: Element<StyledKind>, v: Length): this;
    /** Sets the minimum width and height together. */
    min_size(this: Element<StyledKind>, v: Length): this;
    /** Sets the maximum width. */
    max_w(this: Element<StyledKind>, v: Length): this;
    /** Sets the maximum height. */
    max_h(this: Element<StyledKind>, v: Length): this;
    /** Sets the maximum width and height together. */
    max_size(this: Element<StyledKind>, v: Length): this;
    /** Sets the padding on all four sides. */
    p(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the left and right. */
    px(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the top and bottom. */
    py(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the top. */
    pt(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the bottom. */
    pb(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the left. */
    pl(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the padding on the right. */
    pr(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the margin on all four sides. */
    m(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the left and right. */
    mx(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the top and bottom. */
    my(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the top. */
    mt(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the bottom. */
    mb(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the left. */
    ml(this: Element<StyledKind>, v: Length): this;
    /** Sets the margin on the right. */
    mr(this: Element<StyledKind>, v: Length): this;
    /** Sets all four offsets of a positioned element. */
    inset(this: Element<StyledKind>, v: Length): this;
    /** Sets the top offset of a positioned element. */
    top(this: Element<StyledKind>, v: Length): this;
    /** Sets the bottom offset of a positioned element. */
    bottom(this: Element<StyledKind>, v: Length): this;
    /** Sets the left offset of a positioned element. */
    left(this: Element<StyledKind>, v: Length): this;
    /** Sets the right offset of a positioned element. */
    right(this: Element<StyledKind>, v: Length): this;
    /** Sets the gap between children on both axes. */
    gap(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the gap between columns. */
    gap_x(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets the gap between rows. */
    gap_y(this: Element<StyledKind>, v: DefiniteLength): this;
    /** Sets how much of the free space this child takes, 1 when left out. */
    flex_grow(this: Element<StyledKind>, v?: number): this;
    /** Sets how readily this child gives space back, 1 when left out. */
    flex_shrink(this: Element<StyledKind>, v?: number): this;
    /** Sets the size this child starts from before growing or shrinking. */
    flex_basis(this: Element<StyledKind>, v: Length): this;
    /** Sets the background. */
    bg(this: Element<StyledKind>, v: Color | Background): this;
    /** Sets the text color, which children inherit. */
    text_color(this: Element<StyledKind>, v: Color): this;
    /** Sets the background painted behind the text itself. */
    text_bg(this: Element<StyledKind>, v: Color): this;
    /** Sets the font size. */
    text_size(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the font family. */
    font_family(this: Element<StyledKind>, v: string): this;
    /** Sets the font weight, 100 to 900. */
    font_weight(this: Element<StyledKind>, v: number): this;
    /** Sets the line height. A bare number is a multiplier (`1.45`), a string a length. */
    line_height(this: Element<StyledKind>, v: number | DefiniteLength): this;
    /** Sets the opacity of the element and everything in it, from 0 to 1. */
    opacity(this: Element<StyledKind>, v: number): this;
    /** Sets the border width on all four sides. Draws nothing without a color. */
    border(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the top. */
    border_t(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the bottom. */
    border_b(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the left. */
    border_l(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the right. */
    border_r(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the left and right. */
    border_x(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border width on the top and bottom. */
    border_y(this: Element<StyledKind>, v: AbsoluteLength): this;
    /** Sets the border color. Draws nothing without a width. */
    border_color(this: Element<StyledKind>, v: Color): this;
    /** Sets the corner radius on all four corners. */
    rounded(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the two top corners. */
    rounded_t(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the two bottom corners. */
    rounded_b(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the two left corners. */
    rounded_l(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the two right corners. */
    rounded_r(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the top-left corner. */
    rounded_tl(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the top-right corner. */
    rounded_tr(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the bottom-left corner. */
    rounded_bl(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Sets the corner radius on the bottom-right corner. */
    rounded_br(this: Element<StyledKind>, v: AbsoluteLength | RadiusToken): this;
    /** Adds one child. A string is text; an `Entity` from `cx.new` mounts a retained view. */
    child(this: Element<ParentKind>, child: Element | Entity | string | number | boolean): this;
    /** Adds several children, in order. */
    children(this: Element<ParentKind>, children: Array<Element | Entity | string | number | boolean>): this;
    /** A stable name for this element, its identity across renders. */
    id(this: Element<InteractiveKind>, id: string | number): this;
    /** The key-binding context this element and its subtree sit in. */
    key_context(this: Element<InteractiveKind>, context: string): this;
    /** Tracks a focus handle from `cx.focus_handle()`, so it can focus this element. */
    track_focus(this: Element<InteractiveKind>, handle: FocusHandle): this;
    /** Where this element sits in the Tab order; also makes it a tab stop. */
    tab_index(this: Element<InteractiveKind>, index: number): this;
    /** Whether Tab can land on this element. */
    tab_stop(this: Element<InteractiveKind>, value: boolean): this;
    /** Lets a `ScrollHandle` read and drive this element's scroll position. */
    track_scroll(this: Element<InteractiveKind>, handle: ScrollHandle): this;
    /** Scrolls both axes for overflowing children. */
    overflow_scroll(this: Element<InteractiveKind>): this;
    /** Scrolls horizontally for overflowing children. */
    overflow_x_scroll(this: Element<InteractiveKind>): this;
    /** Scrolls vertically for overflowing children. */
    overflow_y_scroll(this: Element<InteractiveKind>): this;
    /** The label a hover shows over this element. */
    tooltip(this: Element<InteractiveKind>, text: string): this;
    /** What this element announces itself as. */
    role(this: Element<InteractiveKind>, name: "alert" | "button" | "checkbox" | "dialog" | "group" | "heading" | "image" | "link" | "list" | "list_item" | "listbox" | "menu" | "menu_item" | "option" | "progress_indicator" | "radio_button" | "slider" | "status" | "switch" | "tab" | "tab_list" | "tab_panel" | "text_input" | "toolbar" | "tooltip"): this;
    /** What a screen reader announces. */
    aria_label(this: Element<InteractiveKind>, label: string): this;
    /** Same as `aria_label`. */
    accessibility_label(this: Element<InteractiveKind>, label: string): this;
    /** Supplementary text a screen reader announces after the label. */
    aria_description(this: Element<InteractiveKind>, text: string): this;
    /** The selected state, announced. */
    aria_selected(this: Element<InteractiveKind>, value: boolean): this;
    /** The expanded state, announced. */
    aria_expanded(this: Element<InteractiveKind>, value: boolean): this;
    /** The heading level, announced. */
    aria_level(this: Element<InteractiveKind>, level: number): this;
    /** Announces this element as focused while an ancestor holds the keyboard. */
    aria_active_descendant(this: Element<InteractiveKind>): this;
    /** Makes the element focusable without tracking a handle. */
    focusable(this: Element<InteractiveKind>, value?: boolean): this;
    /** Blocks the mouse from reaching elements behind this one. */
    occlude(this: Element<InteractiveKind>): this;
    /** Names a group for `group_hover` of descendants. */
    group(this: Element<InteractiveKind>, name: string): this;
    /** `handler(event, cx)` on a click, or keyboard activation. */
    on_click(this: Element<InteractiveKind>, handler: (event: ClickEvent, cx: Context) => void): this;
    /** `handler(hovered, cx)` when the pointer enters and leaves. */
    on_hover(this: Element<InteractiveKind>, handler: (event: boolean, cx: Context) => void): this;
    /** `handler(event, cx)` when `button` is pressed over this element. */
    on_mouse_down(this: Element<InteractiveKind>, button: MouseButton, handler: (event: MouseButtonEvent, cx: Context) => void): this;
    /** `handler(event, cx)` when `button` is released over this element. */
    on_mouse_up(this: Element<InteractiveKind>, button: MouseButton, handler: (event: MouseButtonEvent, cx: Context) => void): this;
    /** `handler(event, cx)` on a press anywhere outside this element. */
    on_mouse_down_out(this: Element<InteractiveKind>, handler: (event: MouseButtonEvent, cx: Context) => void): this;
    /** `handler(event, cx)` while the pointer moves over this element. */
    on_mouse_move(this: Element<InteractiveKind>, handler: (event: MouseMoveEvent, cx: Context) => void): this;
    /** `handler(event, cx)` on wheel and trackpad scrolling over this element. */
    on_scroll_wheel(this: Element<InteractiveKind>, handler: (event: ScrollWheelEvent, cx: Context) => void): this;
    /** `handler(event, cx)` on a key press while this element or a descendant has focus. */
    on_key_down(this: Element<InteractiveKind>, handler: (event: KeyEvent, cx: Context) => void): this;
    /** `handler(event, cx)` on a key release while this element or a descendant has focus. */
    on_key_up(this: Element<InteractiveKind>, handler: (event: KeyEvent, cx: Context) => void): this;
    /** `handler(event, cx)` when a modifier key changes while focused. */
    on_modifiers_changed(this: Element<InteractiveKind>, handler: (event: ModifiersChangedEvent, cx: Context) => void): this;
    /**
     * `handler(event, cx)` when the action named `action` of this plugin is dispatched to this
     * element or inside it, by a key binding (`cx.bind_keys`) or `window.dispatch_action`.
     * Action names are per plugin; `::` is reserved and throws.
     */
    on_action(this: Element<InteractiveKind>, action: string, handler: (event: ActionEvent, cx: Context) => void): this;
    /** Passes this element to `transform` and returns what it returns. */
    map<T>(transform: (element: this) => T): T;
    /** Applies `branch` only when `condition` is truthy. */
    when(condition: unknown, branch: (element: this) => this): this;
    /** Applies `branch` only when `value` is not null or undefined, passing it along. */
    when_some<T>(value: T | null | undefined, branch: (element: this, value: T) => this): this;
    /** Styles while hovered. */
    hover(this: Element<StyledKind>, style: (style: Element<"style">) => Element): this;
    /** Styles while pressed. */
    active(this: Element<StyledKind>, style: (style: Element<"style">) => Element): this;
    /** Styles while focused. */
    focus(this: Element<StyledKind>, style: (style: Element<"style">) => Element): this;
    /** Styles while something inside is focused. */
    in_focus(this: Element<StyledKind>, style: (style: Element<"style">) => Element): this;
    /** Styles while focused from the keyboard. */
    focus_visible(this: Element<StyledKind>, style: (style: Element<"style">) => Element): this;
    /** Styles while an ancestor with `group(name)` is hovered. */
    group_hover(this: Element<StyledKind>, group: string, style: (style: Element<"style">) => Element): this;
    /** Sets the probe's label. */
    label(this: Element<"Probe">, text: string): this;
    /** The visible label of a button, check, switch, radio, toggle, color picker or field. */
    label(this: Element<"Button" | "Checkbox" | "ColorPicker" | "Field" | "InputGroupButton" | "Radio" | "Switch" | "Toggle">, text: string): this;
    /** An icon of an `InputGroupButton`, by asset path. */
    icon(this: Element<"InputGroupButton">, path: string): this;
    /** Concise hover help. */
    tooltip(this: Element<"Button" | "Button@base" | "Checkbox" | "Checkbox@base" | "Clipboard" | "InputGroupButton" | "Switch" | "Switch@base" | "Toggle" | "Toggle@base">, text: string): this;
    /** The controlled value of a `Checkbox`, `Switch`, `Radio` or `Toggle`. */
    checked(this: Element<"Checkbox" | "Checkbox@base" | "Radio" | "Radio@base" | "Switch" | "Switch@base" | "Toggle">, value: boolean): this;
    /** The controlled state of a `corona/base` `Toggle`: a button that stays down. */
    pressed(this: Element<"Toggle@base">, value: boolean): this;
    /**
     * `handler(checked, cx)` with the new value of a check, switch, radio or toggle. The script
     * owns the value.
     */
    on_change(this: Element<"Checkbox" | "Checkbox@base" | "Radio" | "Radio@base" | "Switch" | "Switch@base" | "Toggle" | "Toggle@base">, handler: (event: boolean, cx: Context) => void): this;
    /**
     * `handler(index, cx)` with the index of the checked `Radio` of a `RadioGroup`, or the
     * stars of a `Rating`.
     */
    on_change(this: Element<"RadioGroup" | "Rating">, handler: (event: number, cx: Context) => void): this;
    /** `handler(text, cx)` with the new text of an `Input` or `Textarea`. */
    on_change(this: Element<"Input" | "Textarea">, handler: (event: string, cx: Context) => void): this;
    /** Blocks interaction and draws the control disabled. */
    disabled(this: Element<"Button" | "Button@base" | "Checkbox" | "Checkbox@base" | "Combobox" | "Combobox@base" | "DatePicker" | "DatePicker@base" | "Input" | "InputGroup" | "InputGroupButton" | "Label" | "Link" | "Link@base" | "NumberInput" | "NumberInput@base" | "OtpInput" | "OtpInput@base" | "Radio" | "Radio@base" | "RadioGroup" | "Rating" | "Select" | "Select@base" | "Slider" | "Slider@base" | "SliderThumb@base" | "SliderTrack@base" | "Switch" | "Switch@base" | "Textarea" | "TimeField" | "Toggle" | "Toggle@base">, value: boolean): this;
    /** The selected state of a `Button`. */
    selected(this: Element<"Button" | "Button@base" | "InputGroupButton">, value: boolean): this;
    /** Shows a spinner and blocks clicks. */
    loading(this: Element<"Button" | "InputGroupButton">, value: boolean): this;
    /** The semantic control size. */
    size(this: Element<"Button" | "Button@base" | "Checkbox" | "Checkbox@base" | "Form" | "InputGroup" | "InputGroupButton" | "Radio" | "Radio@base" | "Rating" | "Switch" | "Switch@base" | "Toggle" | "Toggle@base">, size: "xsmall" | "small" | "medium" | "large"): this;
    /** The outlined look of a button, toggle or keycap. */
    outline(this: Element<"Button" | "InputGroupButton" | "Kbd" | "Toggle">): this;
    /** The primary action look. */
    primary(this: Element<"Button">): this;
    /** The secondary look of a `Button`, or the muted secondary text of a `Label`. */
    secondary(this: Element<"Button" | "Label">, text?: string): this;
    /** The destructive action look. */
    danger(this: Element<"Button">): this;
    /** The success look. */
    success(this: Element<"Button">): this;
    /** The warning look. */
    warning(this: Element<"Button">): this;
    /** The quiet look of a button. */
    ghost(this: Element<"Button">): this;
    /** The look of a link. */
    link(this: Element<"Button">): this;
    /** Tighter padding. */
    compact(this: Element<"Button">): this;
    /** A native button variant of an `InputGroupButton`. */
    variant(this: Element<"InputGroupButton">, variant: "default" | "primary" | "secondary" | "danger" | "warning" | "success" | "info" | "ghost" | "link" | "text"): this;
    /** The absolute HTTP(S) URL a `Link` opens. */
    href(this: Element<"Link" | "Link@base">, url: string): this;
    /** The checked radio of a `RadioGroup`, by index. */
    selected_index(this: Element<"RadioGroup">, index: number): this;
    /** The orientation a `corona/base` `RadioGroup` or `ToggleGroup` announces. */
    axis(this: Element<"RadioGroup@base" | "Slider@base" | "SliderIndicator@base" | "SliderThumb@base" | "SliderTrack@base" | "ToggleGroup@base">, axis: "horizontal" | "vertical"): this;
    /** This item's one-based position and its collection's size, announced. */
    set_position(this: Element<"Radio@base">, position: number, size: number): this;
    /** Draws a `Label`'s text as a password. */
    masked(this: Element<"Input" | "Label">, value: boolean): this;
    /** Highlights the matches of `text` in a `Label`. */
    highlights(this: Element<"Label">, text: string): this;
    /** Whether a keycap or text area draws its frame. */
    appearance(this: Element<"Kbd" | "Textarea">, value: boolean): this;
    /**
     * What a `Clipboard` copies, the text of an `Input` or `Textarea`, or the row id a `Select`
     * shows as selected; an id without a row selects nothing.
     */
    value(this: Element<"Clipboard" | "Input" | "Select" | "Textarea">, value: string): this;
    /** The stars of a `Rating`. */
    value(this: Element<"Rating">, stars: number): this;
    /** How many stars a `Rating` has. */
    max(this: Element<"Rating">, count: number): this;
    /** The accent color of a `Rating`. */
    color(this: Element<"Rating">, color: Color): this;
    /** Draws a `Textarea`'s border. */
    bordered(this: Element<"Textarea">, value: boolean): this;
    /** Allows selecting and copying but no edits. */
    readonly(this: Element<"Input" | "InputGroup" | "Textarea">, value: boolean): this;
    /** The prompt shown while empty. */
    placeholder(this: Element<"Combobox" | "DatePicker" | "Input" | "NumberInput" | "Select" | "Textarea">, text: string): this;
    /** The prompt of a `Combobox`'s search field. */
    search_placeholder(this: Element<"Combobox">, text: string): this;
    /** Shows a search field above a `Combobox`'s items; takes effect from the next frame. */
    searchable(this: Element<"Combobox">, value: boolean): this;
    /** The width of a `Select`'s or `Combobox`'s menu, in pixels. */
    menu_width(this: Element<"Combobox" | "Select">, width: number): this;
    /** Splits an `OtpInput`'s cells into this many groups. */
    groups(this: Element<"OtpInput">, count: number): this;
    /** A vertical `Slider`. */
    vertical(this: Element<"Slider" | "SliderIndicator@base" | "SliderThumb@base">): this;
    /** Fills a `Slider` from the other end. */
    reverse(this: Element<"Slider" | "SliderIndicator@base">): this;
    /** Draws an `InputGroup` as invalid. */
    invalid(this: Element<"InputGroup">, value: boolean): this;
    /** Draws the focus ring of an `InputGroup`. */
    focus_ring(this: Element<"InputGroup">, value: boolean): this;
    /** Supporting text under a `Field`'s control. */
    description(this: Element<"Field">, text: string): this;
    /** Marks a `Field` as required. */
    required(this: Element<"Field">, value: boolean): this;
    /** Indents an unlabeled `Field` of a horizontal form like the labeled ones. */
    label_indent(this: Element<"Field">, value: boolean): this;
    /** How many grid columns a `Field` spans. */
    col_span(this: Element<"Field">, span: number): this;
    /** Where a `Field` aligns its control. */
    align(this: Element<"Field">, to: "start" | "center" | "end"): this;
    /** Where an `InputGroupAddon` sits in its group. */
    align(this: Element<"InputGroupAddon">, to: "inline-start" | "inline-end" | "block-start" | "block-end"): this;
    /** How many columns a `Form` lays its fields out in. */
    columns(this: Element<"Form">, count: number): this;
    /** The label width of a horizontal `Form`, in pixels. */
    label_width(this: Element<"Form">, width: number): this;
    /** The text control of an `InputGroup`: an `InputGroupInput` or `InputGroupTextarea`. */
    input(this: Element<"InputGroup">, control: Element<"Input" | "Textarea">): this;
    /** The editor slot of a `corona/base` `NumberInput`. */
    input(this: Element<"NumberInput@base">, control: Element | Entity | string | number | boolean): this;
    /** Appends an `InputGroupAddon` to an `InputGroup`. */
    addon(this: Element<"InputGroup">, addon: Element<"InputGroupAddon">): this;
    /** Names the control for accessibility. */
    aria_label(this: Element<"Input" | "Input@base" | "InputGroup" | "InputGroupButton" | "Textarea" | "Textarea@base">, label: string): this;
    /** What a screen reader announces. */
    accessibility_label(this: Element<"Button" | "Button@base" | "Checkbox" | "Checkbox@base" | "ColorPicker" | "Link" | "Link@base" | "Radio" | "Radio@base" | "Select" | "Select@base" | "Switch" | "Switch@base" | "Toggle" | "Toggle@base">, label: string): this;
    /** Whether Tab lands on a `Radio`. */
    tab_stop(this: Element<"Radio" | "Radio@base">, value: boolean): this;
    /**
     * Renders each inline token: `render(token, cx)` answers its element, `null` for the
     * default chip.
     */
    token(this: Element<"Input" | "Textarea">, render: (token: import("corona/component").InlineTokenContext, cx: Context) => Element | string | null | undefined): this;
    /** `listener(event, cx)` after a click on a token that nothing consumed. */
    on_token_click(this: Element<"Input" | "Textarea">, listener: (event: import("corona/component").InlineTokenClickEvent, cx: Context) => void): this;
    /** `listener(event, cx)` when the pointer enters or leaves a token. */
    on_token_hover(this: Element<"Input" | "Textarea">, listener: (event: import("corona/component").InlineTokenHoverEvent, cx: Context) => void): this;
    /** The kind of text an `Input` holds, for autofill and keyboards. */
    content_type(this: Element<"Input">, kind: "name" | "name_prefix" | "given_name" | "middle_name" | "family_name" | "name_suffix" | "nickname" | "job_title" | "organization_name" | "location" | "full_street_address" | "street_address_line1" | "street_address_line2" | "address_city" | "address_state" | "address_city_and_state" | "sublocality" | "country_name" | "postal_code" | "telephone_number" | "email_address" | "url" | "credit_card_number" | "credit_card_name" | "credit_card_given_name" | "credit_card_middle_name" | "credit_card_family_name" | "credit_card_security_code" | "credit_card_expiration" | "credit_card_expiration_month" | "credit_card_expiration_year" | "credit_card_type" | "username" | "password" | "new_password" | "one_time_code" | "shipment_tracking_number" | "flight_number" | "date_time" | "birthdate" | "birthdate_day" | "birthdate_month" | "birthdate_year" | "cellular_eid" | "cellular_imei"): this;
    /** A developer-assigned accessibility identifier. */
    accessibility_id(this: Element<"Input" | "Textarea">, id: string): this;
    /** The number of rows a `Textarea` shows, on its state. */
    rows(this: Element<"Textarea">, rows: number): this;
    /** Grows a `Textarea` between two row counts, on its state. */
    auto_grow(this: Element<"Textarea">, min_rows: number, max_rows: number): this;
    /** `handler(event, cx)` on activation. */
    on_click(this: Element<"Button" | "Button@base" | "InputGroupButton" | "Link" | "Link@base">, handler: (event: ClickEvent, cx: Context) => void): this;
    /** Whether a `corona/base` `Select`, `Combobox` or `DatePicker` is showing its list. */
    open(this: Element<"Combobox@base" | "DatePicker@base" | "Select@base">, value: boolean): this;
    /** `handler(open, cx)` when something other than the script opened or closed it. */
    on_open_change(this: Element<"Combobox@base" | "DatePicker@base" | "Select@base">, handler: (event: boolean, cx: Context) => void): this;
    /** `handler(_, cx)` on Enter in an open `corona/base` `Select` or `Combobox`. */
    on_confirm(this: Element<"Combobox@base" | "Select@base">, handler: (event: {}, cx: Context) => void): this;
    /** `handler(_, cx)` on Escape in an open `corona/base` `Select` or `Combobox`. */
    on_dismiss(this: Element<"Combobox@base" | "Select@base">, handler: (event: {}, cx: Context) => void): this;
    /** The handle a `corona/base` `Select` or `Combobox` moves the keyboard to when it opens. */
    content_focus_handle(this: Element<"Combobox@base" | "Select@base">, handle: FocusHandle): this;
    /** Which thumb of a range slider a `SliderThumb` is. */
    start(this: Element<"SliderThumb@base">, value: boolean): this;
    /** How the filled part of a `SliderIndicator` looks: `declare(el)` styles it. */
    range_style(this: Element<"SliderIndicator@base">, declare: (style: Element<"style">) => unknown): this;
    /** How every cell of a `corona/base` `OtpInput` looks. */
    cell_style(this: Element<"OtpInput@base">, declare: (style: Element<"style">) => unknown): this;
    /** What the cell the next digit lands in adds to `cell_style`. */
    cell_active_style(this: Element<"OtpInput@base">, declare: (style: Element<"style">) => unknown): this;
    /** The blinking mark in that cell while it is empty. */
    caret_style(this: Element<"OtpInput@base">, declare: (style: Element<"style">) => unknown): this;
    /** The look of a `corona/base` `NumberInput`'s decrement button. */
    decrement_button(this: Element<"NumberInput@base">, look: Element | Entity | string | number | boolean): this;
    /** The look of a `corona/base` `NumberInput`'s increment button. */
    increment_button(this: Element<"NumberInput@base">, look: Element | Entity | string | number | boolean): this;
    /** Stacks both step buttons to the right of the text. */
    controls_right(this: Element<"NumberInput@base">): this;
    /** `handler(action, cx)` on a step; replaces the built-in stepping. */
    on_step(this: Element<"NumberInput@base">, handler: (event: "increment" | "decrement", cx: Context) => void): this;
    /** Whether a `Tab`, sidebar row, header or footer shows as selected. */
    selected(this: Element<"DropdownButton" | "SidebarFooter" | "SidebarHeader" | "SidebarMenuItem" | "Tab" | "Tab@base">, value: boolean): this;
    /** Disables a tab, accordion part, step, pagination, menu or command item. */
    disabled(this: Element<"Accordion" | "AccordionItem" | "AccordionItem@base" | "CommandItem" | "DropdownButton" | "Menu" | "MenuItem" | "NativeMenuItem" | "NativeMenuTrigger" | "Pagination" | "SidebarMenuItem" | "Stepper" | "StepperItem" | "Tab" | "Tab@base" | "Toolbar">, value: boolean): this;
    /** A `Tab`'s one-based place among `size` tabs, announced as "tab 2 of 5". */
    set_position(this: Element<"Tab@base">, position: number, size: number): this;
    /** Whether an accordion item or a collapsible is open. */
    open(this: Element<"AccordionItem" | "AccordionItem@base" | "Collapsible" | "Collapsible@base">, value: boolean): this;
    /** An `AccordionItem`'s `AccordionHeader`. */
    header(this: Element<"AccordionItem@base">, el: Element<"AccordionHeader@base">): this;
    /** A `Sidebar`'s header. */
    header(this: Element<"Sidebar">, el: Element | Entity | string | number | boolean): this;
    /** A `Sidebar`'s footer. */
    footer(this: Element<"Sidebar">, el: Element | Entity | string | number | boolean): this;
    /** An `AccordionItem`'s `AccordionPanel`. */
    panel(this: Element<"AccordionItem@base">, element: Element<"AccordionPanel@base">): this;
    /** What revealing a collapsible shows. */
    content(this: Element<"Collapsible" | "Collapsible@base">, el: Element | Entity | string | number | boolean): this;
    /** A stable identity for a collapsible's reveal motion. */
    motion_id(this: Element<"Collapsible">, id: string | number): this;
    /** An `AccordionHeader`'s announced heading level, 3 by default. */
    aria_level(this: Element<"AccordionHeader@base">, level: number): this;
    /** Keeps a shut `AccordionPanel` in the tree, with its scroll position and focus. */
    keep_mounted(this: Element<"AccordionPanel@base">, value?: boolean): this;
    /** `handler(open, cx)` with the open state an `AccordionTrigger` asks for. */
    on_change(this: Element<"AccordionTrigger@base">, handler: (event: boolean, cx: Context) => void): this;
    /**
     * `handler(index, cx)` with the index a `TabBar` or `Stepper`, or the page a `Pagination`,
     * asks for.
     */
    on_change(this: Element<"Pagination" | "Stepper" | "TabBar">, handler: (event: number, cx: Context) => void): this;
    /** A resizable panel's initial size in pixels. */
    size(this: Element<"ResizablePanel" | "ResizablePanel@base">, pixels: number): this;
    /** A component's semantic size. */
    size(this: Element<"Accordion" | "Accordion@base" | "DropdownButton" | "Pagination" | "Pagination@base" | "Stepper" | "TabBar">, size: "xsmall" | "small" | "medium" | "large"): this;
    /** Whether a `resizable_panel()` is drawn; hidden, it keeps its place. Default `true`. */
    visible(this: Element<"ResizablePanel" | "ResizablePanel@base">, value?: boolean): this;
    /** How far a resizable panel may be dragged, in pixels. */
    size_range(this: Element<"ResizablePanel" | "ResizablePanel@base">, min: number, max?: number): this;
    /** `handler(sizes, cx)` after a drag resized the panels of a resizable group. */
    on_resize(this: Element<"Resizable" | "Resizable@base">, handler: (event: number[], cx: Context) => void): this;
    /** A `Resizable`'s direction: `"horizontal"` or `"vertical"`. */
    axis(this: Element<"Resizable" | "Resizable@base">, value: "horizontal" | "vertical"): this;
    /** A `Resizable`'s size across its axis, in pixels. */
    cross_size(this: Element<"Resizable" | "Resizable@base">, pixels: number): this;
    /** When a `Scrollbar` shows: `"scrolling"`, `"hover"` or `"always"`. */
    mode(this: Element<"Scrollbar">, value: "scrolling" | "hover" | "always"): this;
    /** The content size a `Scrollbar` measures its thumb against, in pixels. */
    scroll_size(this: Element<"Scrollbar">, width: number, height: number): this;
    /** Makes a `Scrollbar` take its viewport from its own bounds. */
    viewport_from_layout(this: Element<"Scrollbar">, enabled?: boolean): this;
    /** Which axes a `Scrollbar` or `Scroll` covers: `"vertical"`, `"horizontal"` or `"both"`. */
    scroll_axis(this: Element<"Scroll" | "Scrollbar">, axis: "vertical" | "horizontal" | "both"): this;
    /** Lets a `VirtualListScrollHandle` drive a virtual list. */
    track_scroll(this: Element<"VirtualList@base">, handle: import("corona/base").VirtualListScrollHandle): this;
    /** Which item a virtual list measures for its cross-axis size. */
    with_item_to_measure_index(this: Element<"VirtualList@base">, index: number): this;
    /** A `Tab`'s text, or a `Separator`'s. */
    label(this: Element<"Separator" | "Tab">, label: string): this;
    /** What a `Tab` announces. */
    aria_label(this: Element<"Tab" | "Tab@base">, label: string): this;
    /** `handler(key, cx)` when a row of a virtual list is clicked, `key` what `get_key` gave. */
    on_item_click(this: Element<"VirtualList@base">, handler: (event: string, cx: Context) => void): this;
    /** `handler(key, event, cx)` on a secondary press over a row of a virtual list. */
    on_item_secondary_click(this: Element<"VirtualList@base">, handler: (key: string, event: ClickEvent, cx: Context) => void): this;
    /** The zero-based selected tab or current step. */
    selected_index(this: Element<"Stepper" | "TabBar">, index: number): this;
    /** A `DropdownButton`'s look. */
    variant(this: Element<"DropdownButton">, variant: "primary" | "secondary" | "danger" | "ghost"): this;
    /** A `GroupBox`'s look. */
    variant(this: Element<"GroupBox">, variant: "normal" | "fill" | "outline"): this;
    /** A `TabBar`'s look. */
    variant(this: Element<"TabBar">, variant: "tab" | "outline" | "pill" | "segmented" | "underline"): this;
    /** Whether a `TabBar` lists tabs that do not fit in a menu. */
    menu(this: Element<"TabBar">, value: boolean): this;
    /** Whether an `Accordion` keeps several items open. */
    multiple(this: Element<"Accordion">, value: boolean): this;
    /** Whether an `Accordion` or `Command` draws its border. */
    bordered(this: Element<"Accordion" | "Command">, value: boolean): this;
    /** `handler(open_indices, cx)` after an `Accordion` item was toggled. */
    on_toggle(this: Element<"Accordion">, handler: (event: number[], cx: Context) => void): this;
    /** An `AccordionItem`'s title row or a `GroupBox`'s title. */
    title(this: Element<"AccordionItem" | "GroupBox">, title: Element | Entity | string | number | boolean): this;
    /** The current one-based page. */
    current_page(this: Element<"Pagination">, page: number): this;
    /** How many pages there are. */
    total_pages(this: Element<"Pagination">, pages: number): this;
    /** How many page buttons show at most. */
    visible_pages(this: Element<"Pagination">, pages: number): this;
    /** Shows only the previous and next buttons. */
    compact(this: Element<"Pagination">): this;
    /** Lays a `Stepper` out top to bottom. */
    vertical(this: Element<"Stepper">, value?: boolean): this;
    /** Centers each step's text in a horizontal `Stepper`. */
    text_center(this: Element<"Stepper">, value?: boolean): this;
    /** A `Separator`'s line color. */
    color(this: Element<"Separator">, color: Color): this;
    /** Draws a `Separator` dashed. */
    dashed(this: Element<"Separator">): this;
    /** Adds to a `StatusBar`'s leading side. */
    left_content(this: Element<"StatusBar">, el: Element | Entity | string | number | boolean): this;
    /** Adds to a `StatusBar`'s trailing side. */
    right_content(this: Element<"StatusBar">, el: Element | Entity | string | number | boolean): this;
    /** The side a `Sidebar` or `SidebarToggleButton` sits on: `"left"` or `"right"`. */
    side(this: Element<"Sidebar" | "SidebarToggleButton">, side: "left" | "right"): this;
    /** How a `Sidebar` collapses: `"icon"`, `"offcanvas"` or `"none"`. */
    collapsible(this: Element<"Sidebar">, mode: "icon" | "offcanvas" | "none"): this;
    /** Whether a `Sidebar` is collapsed, and which icon its toggle shows. */
    collapsed(this: Element<"Sidebar" | "SidebarToggleButton">, value: boolean): this;
    /** `handler(event, cx)` when a sidebar row or toggle is pressed. */
    on_click(this: Element<"DropdownButton" | "SidebarMenuItem" | "SidebarToggleButton">, handler: (event: ClickEvent, cx: Context) => void): this;
    /** Whether a sidebar row's submenu starts open. */
    default_open(this: Element<"SidebarMenuItem">, value: boolean): this;
    /** Lets a press on a sidebar row open its submenu. */
    click_to_open(this: Element<"SidebarMenuItem">, value: boolean): this;
    /** Lets a press on a sidebar row toggle its submenu. */
    click_to_toggle(this: Element<"SidebarMenuItem">, value: boolean): this;
    /** A sidebar row's icon: `"home"`, `"components"`, `"settings"`, `"archive"` or `"account"`. */
    icon(this: Element<"SidebarMenuItem">, icon: "home" | "components" | "settings" | "archive" | "account"): this;
    /** Whether a `Command` shows its search field. */
    searchable(this: Element<"Command">, value: boolean): this;
    /** Whether a `Command` filters its items by the query. */
    filterable(this: Element<"Command">, value: boolean): this;
    /** A `Command`'s search placeholder. */
    placeholder(this: Element<"Command">, text: string): this;
    /** How tall a `Command`'s results grow, in pixels. */
    max_height(this: Element<"Command">, pixels: number): this;
    /** `handler(query, cx)` after a `Command`'s query changed. */
    on_query(this: Element<"Command">, handler: (event: string, cx: Context) => void): this;
    /** `handler({ section, row }, cx)` after a `Command`'s highlight moved. */
    on_select(this: Element<"Command">, handler: (event: { section: number; row: number }, cx: Context) => void): this;
    /** `handler({ section, row }, cx)` after a `Command` item was confirmed. */
    on_confirm(this: Element<"Command">, handler: (event: { section: number; row: number }, cx: Context) => void): this;
    /** `handler(cx)` when Escape is pressed on a `Command` with an empty query. */
    on_cancel(this: Element<"Command">, handler: (cx: Context) => void): this;
    /** Another word a `CommandItem` is found by. */
    keyword(this: Element<"CommandItem">, keyword: string): this;
    /** Whether a command or native menu item shows a check. */
    checked(this: Element<"CommandItem" | "MenuItem" | "NativeMenuItem">, value: boolean): this;
    /** The action a command or native menu item dispatches, for `on_action`. */
    action(this: Element<"CommandItem" | "NativeMenuItem">, action: string): this;
    /** The outlined look of a `DropdownButton`. */
    outline(this: Element<"DropdownButton">): this;
    /** Where a `DropdownButton`'s menu opens. */
    menu_anchor(this: Element<"DropdownButton">, anchor: "top_right" | "bottom_right" | "bottom_left" | "top_left"): this;
    /** Adds a `DropdownButton` menu item calling `handler(cx)`. */
    menu_item(this: Element<"DropdownButton">, label: string, handler: (cx: Context) => void): this;
    /**
     * `handler(message, cx)` when the native menu cannot be shown. Showing one cannot fail
     * on this platform layer, so it is kept and never called.
     */
    on_effect_error(this: Element<"NativeMenuTrigger">, handler: (event: string, cx: Context) => void): this;
    /** Adds a `DropdownMenu` item calling `handler(cx)`. */
    item(this: Element<"DropdownMenu">, label: string, handler: (cx: Context) => void): this;
    /**
     * A stable name for this element; a scrolling one is what a `Scrollbar` of that name
     * drives.
     */
    id(this: Element<"div">, id: string | number): this;
    /** Scrolls both axes for overflowing children. */
    overflow_scroll(this: Element<"div">): this;
    /** Scrolls horizontally for overflowing children. */
    overflow_x_scroll(this: Element<"div">): this;
    /** Scrolls vertically for overflowing children. */
    overflow_y_scroll(this: Element<"div">): this;
    /** Lets a `ScrollHandle` drive this element; a `Scrollbar` of its name drives it too. */
    track_scroll(this: Element<"div">, handle: ScrollHandle): this;
    /** Scrolls both axes and paints base-layer scrollbars. */
    overflow_scrollbar(this: Element<"div">): this;
    /** Scrolls horizontally and paints a base-layer scrollbar. */
    overflow_x_scrollbar(this: Element<"div">): this;
    /** Scrolls vertically and paints a base-layer scrollbar. */
    overflow_y_scrollbar(this: Element<"div">): this;
    /** The element a `corona/base` `Popover` or `HoverCard` draws while closed, and that opens it. */
    trigger(this: Element<"HoverCard@base" | "Popover@base">, element: Element | Entity | string | number | boolean): this;
    /** The element that opens a `corona/component` `HoverCard` when hovered. */
    trigger_element(this: Element<"HoverCard">, element: Element | Entity | string | number | boolean): this;
    /** What a `Popover`, `HoverCard` or `Popup` shows while open: an element of this render. */
    content(this: Element<"HoverCard" | "HoverCard@base" | "Popover" | "Popover@base" | "Popup@base">, content: Element): this;
    /**
     * What a `Dialog` or `Sheet` shows: it outlives the render, so it takes a fn rendering its
     * content instead.
     */
    content(this: Element<"Dialog" | "Sheet">, content: () => Element): this;
    /** Whether a `Popover` is open; set, the script holds the state through `on_open_change`. */
    open(this: Element<"Popover" | "Popover@base">, value: boolean): this;
    /** Whether an uncontrolled `Popover` starts open. */
    default_open(this: Element<"Popover" | "Popover@base">, value: boolean): this;
    /** Whether a press outside an open `Popover` closes it. Default `true`. */
    overlay_closable(this: Element<"Popover" | "Popover@base">, value: boolean): this;
    /** Which pointer button opens a `Popover`. Default `"left"`. */
    mouse_button(this: Element<"Popover" | "Popover@base">, button: "left" | "right" | "middle"): this;
    /** Which corner of a `Popover`, `HoverCard` or `Popup` is pinned to its trigger. */
    anchor(this: Element<"HoverCard" | "HoverCard@base" | "Popover" | "Popover@base" | "Popup@base">, anchor: "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right" | "left_center" | "right_center"): this;
    /** Same as `anchor`. */
    card_anchor(this: Element<"HoverCard" | "HoverCard@base" | "Popover" | "Popover@base" | "Popup@base">, anchor: "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right" | "left_center" | "right_center"): this;
    /** How long the pointer rests on a `HoverCard`'s trigger before it opens. Default 600. */
    open_delay(this: Element<"HoverCard" | "HoverCard@base">, ms: number): this;
    /** How long a `HoverCard` stays open after the pointer left it and its trigger. Default 300. */
    close_delay(this: Element<"HoverCard" | "HoverCard@base">, ms: number): this;
    /** Whether a `corona/component` `Popover` or `HoverCard` draws its surface. Default `true`. */
    appearance(this: Element<"HoverCard" | "Popover">, value: boolean): this;
    /** `handler(open, cx)` when a `Popover` or `HoverCard` opens or closes by itself. */
    on_open_change(this: Element<"HoverCard" | "HoverCard@base" | "Popover" | "Popover@base">, handler: (event: boolean, cx: Context) => void): this;
    /** The title of a window effect or an `Alert`. */
    title(this: Element<"Alert" | "AlertDialog" | "Dialog" | "Notification" | "Sheet">, text: string): this;
    /** The text under an `AlertDialog`'s title. */
    description(this: Element<"AlertDialog">, text: string): this;
    /** The text under a `Notification`'s title. */
    message(this: Element<"Notification">, text: string): this;
    /** Whether an `AlertDialog` has a cancel button. Default `false`. */
    show_cancel(this: Element<"AlertDialog">, value: boolean): this;
    /** The window edge a `Sheet` slides in from. Default `"right"`. */
    placement(this: Element<"Sheet">, at: "top" | "right" | "bottom" | "left"): this;
    /** Whether a `Notification` hides by itself. Default `true`. */
    autohide(this: Element<"Notification">, value: boolean): this;
    /** `handler(cx)` on a `Dialog`'s or `AlertDialog`'s OK button; the dialog closes after. */
    on_ok(this: Element<"AlertDialog" | "Dialog">, handler: (cx: Context) => void): this;
    /** `handler(cx)` on a `Dialog`'s or `AlertDialog`'s cancel button; the dialog closes after. */
    on_cancel(this: Element<"AlertDialog" | "Dialog">, handler: (cx: Context) => void): this;
    /** `handler(cx)` once a window effect closed, however it was closed. */
    on_close(this: Element<"AlertDialog" | "Dialog" | "Notification" | "Sheet">, handler: (cx: Context) => void): this;
    /** `handler(cx)` on a click on a `Notification`. */
    on_click(this: Element<"Notification">, handler: (cx: Context) => void): this;
    /** Draws an `Alert` full width, without border, radius or title. */
    banner(this: Element<"Alert">): this;
    /** Whether an `Alert` is drawn. Default `true`. */
    visible(this: Element<"Alert">, value?: boolean): this;
    /** An `Alert`'s semantic size. */
    size(this: Element<"Alert">, size: "xsmall" | "small" | "medium" | "large"): this;
    /** The kind of a `Notification`. Default `"info"`. */
    type(this: Element<"Notification">, level: "info" | "success" | "warning" | "error"): this;
    /**
     * The value of a `Progress`: a percentage (`corona/base` announces it, `corona/component`
     * draws it, `0..=100`).
     */
    value(this: Element<"Progress" | "Progress@base">, value: number): this;
    /** The text of a `DescriptionItem`. */
    value(this: Element<"DescriptionItem">, value: number | string): this;
    /** Withdraws a `corona/base` `Progress` value from the accessibility tree: "no idea how far". */
    indeterminate(this: Element<"Progress@base">, value: boolean): this;
    /** Fills an `Avatar`'s image slot with an `AvatarImage`; it renders instead of the fallback. */
    image(this: Element<"Avatar@base">, image: Element<"AvatarImage@base">): this;
    /** Fills an `Avatar`'s fallback slot with an `AvatarFallback`, shown without an image. */
    fallback(this: Element<"Avatar@base">, fallback: Element<"AvatarFallback@base">): this;
    /** A `Table`'s total number of rows, also those not rendered, as announced. */
    row_count(this: Element<"Table@base">, count: number): this;
    /** A `Table`'s total number of columns, also those not rendered. */
    column_count(this: Element<"Table@base">, count: number): this;
    /** The semantic size of a styled widget. */
    size(this: Element<"Avatar" | "Avatar@base" | "Badge" | "Calendar" | "CarouselNext" | "CarouselPaginationItem" | "CarouselPrevious" | "DescriptionList" | "Icon" | "Progress" | "Progress@base" | "Spinner" | "Table" | "Table@base" | "Tag">, size: "xsmall" | "small" | "medium" | "large"): this;
    /** What a screen reader announces. */
    accessibility_label(this: Element<"Carousel" | "CarouselItem" | "CarouselNext" | "CarouselPagination" | "CarouselPaginationItem" | "CarouselPrevious" | "Progress" | "Progress@base" | "Table" | "Table@base">, label: string): this;
    /** How many columns a table cell spans. */
    col_span(this: Element<"TableCell" | "TableHead">, span: number): this;
    /** Centers a table cell's text. */
    text_center(this: Element<"TableCell" | "TableCell@base" | "TableHead" | "TableHead@base">): this;
    /** Right-aligns a table cell's text. */
    text_right(this: Element<"TableCell" | "TableCell@base" | "TableHead" | "TableHead@base">): this;
    /** The color of a `Badge`, `Spinner`, `Icon` or `Progress`. */
    color(this: Element<"Badge" | "Icon" | "Progress" | "Spinner">, color: Color): this;
    /** A `Badge` as a dot; a `LineChart` or `RadarChart` with dots on its points. */
    dot(this: Element<"Badge" | "LineChart" | "RadarChart">): this;
    /** The number a `Badge` shows. */
    count(this: Element<"Badge">, count: number): this;
    /** The largest number a `Badge` shows, `max+` above. */
    max(this: Element<"Badge">, max: number): this;
    /** The look of a `Tag`. */
    variant(this: Element<"Tag">, variant: "primary" | "secondary" | "danger" | "success" | "warning" | "info"): this;
    /** The look of an `EmptyMedia`. */
    variant(this: Element<"EmptyMedia">, variant: "default" | "icon"): this;
    /** Draws a `Tag` outlined. */
    outline(this: Element<"Tag">): this;
    /** The quieter color of a `Skeleton`. */
    secondary(this: Element<"Skeleton">): this;
    /** The icon a `Spinner` turns. */
    icon(this: Element<"Spinner">, icon: "loader" | "loader_circle"): this;
    /** How a `Spinner` turns. */
    ease(this: Element<"Spinner">, ease: "linear" | "ease_in_out" | "ease_out_quint"): this;
    /** Fills an `Empty`'s header with an `EmptyHeader`. */
    header(this: Element<"Empty">, header: Element<"EmptyHeader">): this;
    /** Fills an `Empty`'s content with an `EmptyContent`. */
    content(this: Element<"Empty">, content: Element<"EmptyContent">): this;
    /** Fills an `EmptyHeader`'s media with an `EmptyMedia`. */
    media(this: Element<"EmptyHeader">, media: Element<"EmptyMedia">): this;
    /** Fills an `EmptyHeader`'s title with an `EmptyTitle`. */
    title(this: Element<"EmptyHeader">, title: Element<"EmptyTitle">): this;
    /** Fills an `EmptyHeader`'s description with an `EmptyDescription`. */
    description(this: Element<"EmptyHeader">, description: Element<"EmptyDescription">): this;
    /** Rotates an `Icon`. */
    rotate(this: Element<"Icon">, radians: number): this;
    /** Animates a `corona/component` `Progress` as busy. */
    loading(this: Element<"Progress">, value: boolean): this;
    /** The name an `Avatar` shows the initials of without an image. */
    name(this: Element<"Avatar">, name: string): this;
    /** The image of an `Avatar`: an absolute path under a read grant, or a plugin file. */
    src(this: Element<"Avatar" | "Avatar@base">, path: string): this;
    /** Lays a `DescriptionList` out in a column. */
    vertical(this: Element<"DescriptionList">): this;
    /** Draws a `DescriptionList` or `DataTable` with borders. */
    bordered(this: Element<"DataTable" | "DescriptionList">, value: boolean): this;
    /** How many columns a `DescriptionList` has. */
    columns(this: Element<"DescriptionList">, columns: number): this;
    /** Adds `DescriptionItem`s to a `DescriptionList`. */
    items(this: Element<"DescriptionList">, items: Element<"DescriptionItem">[]): this;
    /** Adds `TreeItem`s to a `Tree` or `TreeItem`. */
    items(this: Element<"Tree" | "TreeItem">, items: Element<"TreeItem">[]): this;
    /** Adds a separator row to a `DescriptionList`. */
    separator(this: Element<"DescriptionList">): this;
    /** How many columns a `DescriptionItem` spans. */
    span(this: Element<"DescriptionItem">, span: number): this;
    /** Whether a `TreeItem` starts expanded. */
    expanded(this: Element<"TreeItem">, value: boolean): this;
    /** Whether a `TreeItem` is disabled. */
    disabled(this: Element<"TreeItem">, value: boolean): this;
    /** Draws a chart's grid lines. */
    grid(this: Element<"AreaChart" | "BarChart" | "LineChart" | "RadarChart">, value?: boolean): this;
    /** Draws a `BarChart`'s label axis. */
    label_axis(this: Element<"BarChart">, value: boolean): this;
    /** Draws a `BarChart`'s value axis. */
    value_axis(this: Element<"BarChart">, value: boolean): this;
    /** Draws a `LineChart`'s or `AreaChart`'s x axis. */
    x_axis(this: Element<"AreaChart" | "LineChart">, value: boolean): this;
    /** Labels every `margin`th tick of a chart's axis. */
    tick_margin(this: Element<"AreaChart" | "BarChart" | "LineChart">, margin: number): this;
    /** Draws a smooth curve. */
    natural(this: Element<"AreaChart" | "LineChart">): this;
    /** Draws straight segments. */
    linear(this: Element<"AreaChart" | "LineChart">): this;
    /** Draws steps. */
    step_after(this: Element<"AreaChart" | "LineChart">): this;
    /** A `PieChart`'s hole, in pixels: a donut. */
    inner_radius(this: Element<"PieChart">, radius: number): this;
    /** The gap between a `PieChart`'s slices, in radians. */
    pad_angle(this: Element<"PieChart">, angle: number): this;
    /** Labels a `PieChart`'s slices. */
    labels(this: Element<"PieChart">, value: boolean): this;
    /** How many rings a `RadarChart`'s grid has. */
    grid_levels(this: Element<"RadarChart">, levels: number): this;
    /** How many months a `Calendar` shows side by side. */
    number_of_months(this: Element<"Calendar">, count: number): this;
    /** Stripes a `DataTable`'s rows. */
    stripe(this: Element<"DataTable">, value: boolean): this;
    /** When a `DataTable` shows its scrollbars. */
    scrollbar_visible(this: Element<"DataTable">, vertical: boolean, horizontal: boolean): this;
    /** Whether a `DataTable`'s rows can be selected. */
    row_selectable(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable`'s columns can be selected. */
    column_selectable(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable`'s cells can be selected. */
    cell_selectable(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable` has a row header column. */
    row_header(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable`'s columns sort. */
    sortable(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable`'s columns resize. */
    column_resizable(this: Element<"DataTable">, value: boolean): this;
    /** Whether a `DataTable`'s columns move. */
    column_movable(this: Element<"DataTable">, value: boolean): this;
    /** The semantic size of an `Attachment` or `Settings`. */
    size(this: Element<"Attachment" | "Settings">, size: "xsmall" | "small" | "medium" | "large"): this;
    /** The attachment's lifecycle status. */
    status(this: Element<"Attachment">, status: "pending" | "uploading" | "processing" | "failed" | "complete"): this;
    /** The attachment's layout axis. */
    axis(this: Element<"Attachment">, axis: "horizontal" | "vertical"): this;
    /** The sender edge a `Message` or `Bubble` sits on. */
    alignment(this: Element<"Bubble" | "Message">, alignment: "start" | "end"): this;
    /** The treatment of a `Bubble`. */
    variant(this: Element<"Bubble">, variant: "filled" | "secondary" | "muted" | "tinted" | "outline" | "ghost" | "destructive"): this;
    /** The treatment of a `Marker`. */
    variant(this: Element<"Marker">, variant: "plain" | "separator" | "border"): this;
    /** Whether the marker shows its loading treatment. */
    loading(this: Element<"Marker">, loading: boolean): this;
    /** The marker's loading treatment. */
    loading_style(this: Element<"Marker">, loading_style: "spinner" | "shimmer"): this;
    /** A stable identity for a `ShimmerText`'s animation. */
    id(this: Element<"ShimmerText">, id: string | number): this;
    /** How long one shimmer sweep takes. */
    duration_ms(this: Element<"ShimmerText">, duration_ms: number): this;
    /** The highlight's half-width, relative to the text. */
    spread(this: Element<"ShimmerText">, spread: number): this;
    /** Sweeps right to left. */
    reverse(this: Element<"ShimmerText">, reverse: boolean): this;
    /** Sweeps once instead of looping. */
    once(this: Element<"ShimmerText">, once: boolean): this;
    /** Shows the transcript's scrollbar. */
    scrollbar(this: Element<"MessageScroller">, scrollbar: boolean): this;
    /** Shows the jump-to-latest button. */
    jump_button(this: Element<"MessageScroller">, jump_button: boolean): this;
    /** The jump-to-latest button's label. */
    jump_button_label(this: Element<"MessageScroller">, label: string): this;
    /** Disables an `Editor`, a `SettingItem`'s field, a questionnaire choice or question. */
    disabled(this: Element<"Editor" | "QuestionnaireChoice" | "QuestionnaireInput" | "QuestionnaireItem" | "SettingItem">, disabled: boolean): this;
    /** Draws the editor's own background and border. */
    appearance(this: Element<"Editor">, appearance: boolean): this;
    bordered(this: Element<"Editor">, bordered: boolean): this;
    readonly(this: Element<"Editor">, readonly: boolean): this;
    /** What a screen reader announces. */
    aria_label(this: Element<"Editor">, label: string): this;
    /** Shows line numbers. */
    line_number(this: Element<"Diff">, value: boolean): this;
    syntax_highlight(this: Element<"Diff">, value: boolean): this;
    /** Shows a header above each file. */
    header_visible(this: Element<"Diff">, value: boolean): this;
    /** Tints changed lines. */
    change_background(this: Element<"Diff">, value: boolean): this;
    /** Wraps long lines. */
    soft_wrap(this: Element<"Diff">, value: boolean): this;
    /** How the start of each hunk is marked. */
    hunk_separator(this: Element<"Diff">, separator: "metadata" | "line_info" | "simple"): this;
    /** How changed lines are marked beside the code. */
    change_indicator(this: Element<"Diff">, indicator: "signs" | "bars" | "none"): this;
    /** The text under a setting's or question's title. */
    description(this: Element<"QuestionnaireChoice" | "QuestionnaireItem" | "SettingGroup" | "SettingItem" | "SettingPage">, text: string): this;
    /** The axis a setting's label and field are laid out along. */
    layout(this: Element<"SettingItem">, axis: "horizontal" | "vertical"): this;
    /** Search keywords matching the setting. */
    keywords(this: Element<"SettingItem">, keywords: string[]): this;
    /** The setting group's title. */
    title(this: Element<"SettingGroup">, text: string): this;
    /** Renders a `SettingItem`'s field, or what follows a `SettingPage`'s title, lazily. */
    content(this: Element<"SettingItem" | "SettingPage">, render: (cx: Context) => Element | string | null | undefined): this;
    /** Opens the page's groups. */
    default_open(this: Element<"SettingPage">, default_open: boolean): this;
    /** Shows the reset control. */
    resettable(this: Element<"SettingPage">, resettable: boolean): this;
    /** The sidebar's width in pixels. */
    sidebar_width(this: Element<"Settings">, pixels: number): this;
    /** How far the sidebar can be resized, in pixels. */
    sidebar_size_range(this: Element<"Settings">, minimum: number, maximum: number): this;
    /** The page shown first. */
    default_selected_page(this: Element<"Settings">, index: number): this;
    /** Selects the choice at first. */
    default_selected(this: Element<"QuestionnaireChoice">, value: boolean): this;
    /** Requires an answer; hides Skip. */
    required(this: Element<"QuestionnaireItem">, value: boolean): this;
    /** Accepts several choices. */
    multiple(this: Element<"QuestionnaireItem">, value: boolean): this;
    /** Gives each choice of the active question a letter or number shortcut. */
    shortcuts(this: Element<"Questionnaire">, mode: "letters" | "numbers"): this;
    /** `handler(url, cx)` on a link click, instead of opening it. */
    on_link_click(this: Element<"TextView@base">, handler: (event: string, cx: Context) => void): this;
    /** Whether the text can be selected; on when left out. */
    selectable(this: Element<"TextView@base">, value?: boolean): this;
    /** Whether the view scrolls its content; on when left out. */
    scrollable(this: Element<"TextView@base">, value?: boolean): this;
    /** Where in its `relative()` parent the HUD sits. */
    anchor(this: Element<"fps_monitor">, anchor: "top_left" | "top_right" | "bottom_left" | "bottom_right" | "top_center" | "bottom_center" | "left_center" | "right_center"): this;
    /** The frame budget in milliseconds the HUD grades against. */
    frame_budget(this: Element<"fps_monitor">, ms: number): this;
    /** The tab bar above a group's displayed panel. */
    tab_bar(this: Element<"dock_area@base">, handler: (group: import("corona/base").DockGroup, cx: Context) => Element): this;
    /** What a group with no displayed panel shows. */
    empty_group(this: Element<"dock_area@base">, handler: (group: import("corona/base").DockGroup, cx: Context) => Element | null): this;
    /** The hint where a dragged panel would land. */
    drop_indicator(this: Element<"dock_area@base">, handler: (drop: import("corona/base").DockDrop, cx: Context) => Element | null): this;
    /** One dock's chrome around its content; put `dock_content()` where its panels go. */
    dock(this: Element<"dock_area@base">, handler: (dock: import("corona/base").DockRegion, cx: Context) => Element | null): this;
    /** Displays tab `index` of `group` on a click. */
    select_tab(this: Element<InteractiveKind>, group: import("corona/base").DockGroup, index: number): this;
    /** Closes `panel` (an id) of `group` on a click, if the group allows it. */
    close_panel(this: Element<InteractiveKind>, group: import("corona/base").DockGroup, panel: number): this;
    /** Zooms `group` in, or back out, on a click. */
    toggle_zoom(this: Element<InteractiveKind>, group: import("corona/base").DockGroup): this;
    /** Makes this element the drag source of tab `index` of `group`. */
    drag_tab(this: Element<InteractiveKind>, group: import("corona/base").DockGroup, index: number): this;
    /** Accepts a dragged panel into `group`, at slot `index` or appended. */
    drop_tab(this: Element<InteractiveKind>, group: import("corona/base").DockGroup, index?: number): this;
    /** Opens or closes `dock` on a click. */
    toggle_dock(this: Element<InteractiveKind>, dock: import("corona/base").DockRegion): this;
    /** Drags `dock`'s edge. */
    resize_dock(this: Element<InteractiveKind>, dock: import("corona/base").DockRegion): this;
    grid(this: Element<StyledKind>): this;
    text_center(this: Element<StyledKind>): this;
    text_right(this: Element<StyledKind>): this;
    visible(this: Element<StyledKind>): this;
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
    /** The action it dispatches, handled by `on_action`. Names are per plugin; `::` is reserved and throws. */
    action: string;
    /** A key-context predicate matched against `key_context`; defaults to this plugin's windows, so the binding fires nowhere else. */
    context?: string;
  }
  /** What a view's methods, handlers, timers and tasks get. */
  export interface Context {
    /** Renders the view again, or the retained child `target`. Throws in `render`. */
    notify(target?: Entity): void;
    /** Installs key bindings, all or none; returns how many. They end with the plugin's runtime, so a reload replaces them. */
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
  export function div(): Element<"div">;
  /** A vector icon tinted with the text color: an absolute path or an asset. */
  export function svg(path: string): Element<"svg">;
  /** A full-color image: a file path or PNG, JPEG, WebP, GIF, BMP, TIFF or ICO bytes. */
  export function image(source: string | Uint8Array): Element<"image">;
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
  ): Element<"list">;
  /** gpui's list of rows of one height, rendered a range at a time. */
  export function uniform_list(
    id: string,
    item_count: number,
    get_key: (index: number) => string,
    render: (range: ItemRange, cx: Context) => Element[],
  ): Element<"uniform_list">;
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
    /** Hides after 5 s; `null` keeps it up until removed. */
    timeout?: null;
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
    paint_path(path: Path, fill: Color | Background): Element<"canvas">;
  }
}
declare var window: import("corona/kit").Window;
declare module "corona/base" {
  import { ClickEvent, Context, Element, FocusHandle } from "corona/kit";
  /** A row, centered on the cross axis. */
  export function h_flex(): Element<"div">;
  /** A column. */
  export function v_flex(): Element<"div">;
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
  export const Button: { new (id: string | number): Element<"Button@base">; new: (id: string | number) => Element<"Button@base"> };
  /** An external HTTP(S) resource opened through the system browser. */
  export const Link: { new (id: string | number): Element<"Link@base">; new: (id: string | number) => Element<"Link@base"> };
  /** A controlled toggle. No styling: draw the indicator yourself. */
  export const Checkbox: { new (id: string | number): Element<"Checkbox@base">; new: (id: string | number) => Element<"Checkbox@base"> };
  /** A controlled switch. No styling. */
  export const Switch: { new (id: string | number): Element<"Switch@base">; new: (id: string | number) => Element<"Switch@base"> };
  /** One option in a radio group. No styling. */
  export const Radio: { new (id: string | number): Element<"Radio@base">; new: (id: string | number) => Element<"Radio@base"> };
  /** A button that stays down: `pressed(..)` in, `on_change(..)` out. */
  export const Toggle: { new (id: string | number): Element<"Toggle@base">; new: (id: string | number) => Element<"Toggle@base"> };
  /** A set of radios, announced as one group. */
  export const RadioGroup: { new (id: string | number): Element<"RadioGroup@base">; new: (id: string | number) => Element<"RadioGroup@base"> };
  /** A set of toggles, announced as a toolbar. */
  export const ToggleGroup: { new (id: string | number): Element<"ToggleGroup@base">; new: (id: string | number) => Element<"ToggleGroup@base"> };
  /** A select root: the semantics and the keyboard, none of the picture. */
  export const Select: { new (id: string | number): Element<"Select@base">; new: (id: string | number) => Element<"Select@base"> };
  /** A combobox root whose trigger is an editable field. */
  export const Combobox: { new (id: string | number): Element<"Combobox@base">; new: (id: string | number) => Element<"Combobox@base"> };
  /** A date-picker root; it holds no date. */
  export const DatePicker: { new (id: string | number, focus_handle: FocusHandle): Element<"DatePicker@base">; new: (id: string | number, focus_handle: FocusHandle) => Element<"DatePicker@base"> };
  /** The bare editor of retained text. */
  export const Input: { new (state: InputState): Element<"Input@base">; new: (state: InputState) => Element<"Input@base"> };
  /** A spinbutton; give it `decrement_button`/`increment_button`. */
  export const NumberInput: { new (state: InputState): Element<"NumberInput@base">; new: (state: InputState) => Element<"NumberInput@base"> };
  /** The bare editor of retained multi-line text. */
  export const Textarea: { new (state: TextareaState): Element<"Textarea@base">; new: (state: TextareaState) => Element<"Textarea@base"> };
  /** A code drawn cell by cell: `cell_style(..)`, `caret_style(..)`. */
  export const OtpInput: { new (state: OtpState): Element<"OtpInput@base">; new: (state: OtpState) => Element<"OtpInput@base"> };
  /** The slider root: announces the value. */
  export const Slider: { new (state: SliderState): Element<"Slider@base">; new: (state: SliderState) => Element<"Slider@base"> };
  /** The press and drag surface. */
  export const SliderTrack: { new (state: SliderState): Element<"SliderTrack@base">; new: (state: SliderState) => Element<"SliderTrack@base"> };
  /** The groove; `range_style(..)` draws the fill. */
  export const SliderIndicator: { new (state: SliderState): Element<"SliderIndicator@base">; new: (state: SliderState) => Element<"SliderIndicator@base"> };
  /** The knob; `start(true)` is a range's lower one. */
  export const SliderThumb: { new (state: SliderState): Element<"SliderThumb@base">; new: (state: SliderState) => Element<"SliderThumb@base"> };
  import { InputState, TextareaState, SliderState, OtpState, SliderValue, TextEvent } from "corona/component";
  export { InputState, TextareaState, SliderState, OtpState, SliderValue, TextEvent };
  type Ctor<K extends string, A extends any[]> = { new (...args: A): Element<K>; new: (...args: A) => Element<K> };
  /** A tab list; each `Tab` is told whether it is selected and reports clicks. */
  export const Tabs: Ctor<"Tabs@base", [id: string | number]>;
  /** One tab: `selected(..)` in, `on_click(..)` out. */
  export const Tab: Ctor<"Tab@base", [id: string | number]>;
  /** An accordion: items, each with an `AccordionHeader` and an `AccordionPanel`. Draws nothing. */
  export const Accordion: Ctor<"Accordion@base", [id: string | number]>;
  /** One item: `open(..)`, `header(..)`, `panel(..)`. */
  export const AccordionItem: Ctor<"AccordionItem@base", []>;
  /** The heading owning an item's trigger. */
  export const AccordionHeader: Ctor<"AccordionHeader@base", [trigger: Element<"AccordionTrigger@base">]>;
  /** The region an item reveals; gone while shut unless `keep_mounted()`. */
  export const AccordionPanel: Ctor<"AccordionPanel@base", []>;
  /** The button: `on_change(open => ..)` asks for the other state. */
  export const AccordionTrigger: Ctor<"AccordionTrigger@base", [id: string | number]>;
  /** A pagination landmark; draw the pages from `pagination_items`. */
  export const Pagination: Ctor<"Pagination@base", [id: string | number]>;
  /** One entry of a page layout: a page, or a gap standing for the pages `[first, last]`. */
  export type PaginationEntry =
    | { page: number; ellipsis?: undefined }
    | { ellipsis: [first: number, last: number]; page?: undefined };
  /** Which pages to draw around `current_page`, and where the gaps fall. */
  export function pagination_items(current_page: number, total_pages: number, visible_pages?: number): PaginationEntry[];
  /** Renders its `content` only while `open`; children always. */
  export const Collapsible: Ctor<"Collapsible@base", []>;
  /** A row of `resizable_panel()`s with draggable dividers; sizes persist under `id`. */
  export function h_resizable(id: string | number): Element<"Resizable@base">;
  /** A column of `resizable_panel()`s. */
  export function v_resizable(id: string | number): Element<"Resizable@base">;
  /** One pane of a resizable group: `size(px)`, `size_range(min, max)`. */
  export function resizable_panel(): Element<"ResizablePanel@base">;
  /** A scroll area's or virtual list's name, or its handle. */
  type ScrollTarget = string | number | import("corona/kit").ScrollHandle | VirtualListScrollHandle;
  /** A scrollbar driving the area that tracks `handle`; it fills the element it is put in. */
  export const Scrollbar: {
    new: (handle: ScrollTarget) => Element<"Scrollbar@base">;
    horizontal: (handle: ScrollTarget) => Element<"Scrollbar@base">;
    vertical: (handle: ScrollTarget) => Element<"Scrollbar@base">;
  };
  /** A list of rows of the given heights that renders only the visible ones. */
  export function v_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element<"VirtualList@base">;
  /** `v_virtual_list` along the other axis; `item_sizes` are widths. */
  export function h_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element<"VirtualList@base">;
  /** A virtual list's scroll position; `track_scroll(handle)` on the list. */
  export class VirtualListScrollHandle {
    constructor();
    static new(): VirtualListScrollHandle;
    /** Puts item `index` on screen. */
    scroll_to_item(index: number, strategy?: "top" | "center"): void;
    scroll_to_bottom(): void;
  }
  /** An unstyled surface anchored to `trigger(..)`, opened by a press: `content(..)`, `open(..)`, `on_open_change(..)`. */
  export const Popover: { new (id: string | number): Element<"Popover@base">; new: (id: string | number) => Element<"Popover@base"> };
  /** An unstyled surface anchored to `trigger(..)`, opened by resting the pointer on it. */
  export const HoverCard: { new (id: string | number): Element<"HoverCard@base">; new: (id: string | number) => Element<"HoverCard@base"> };
  /** The bare anchored surface: shows `content(..)` while it is set, anchored to `trigger`. */
  export const Popup: { new (id: string | number, trigger: Element): Element<"Popup@base">; new: (id: string | number, trigger: Element) => Element<"Popup@base"> };
  /** Takes no space, not even a gap: return it to hide a widget or card. */
  export const Empty: { new (): Element<"Empty@base">; new: () => Element<"Empty@base"> };
  /** The progress root: announces `value`, draws nothing; put a `ProgressTrack` in it. */
  export const Progress: { new (id: string | number): Element<"Progress@base">; new: (id: string | number) => Element<"Progress@base"> };
  /** The groove, a plain element. */
  export const ProgressTrack: { new (): Element<"ProgressTrack@base">; new: () => Element<"ProgressTrack@base"> };
  /** The filled part, a plain element: size it from the value. */
  export const ProgressIndicator: { new (): Element<"ProgressIndicator@base">; new: () => Element<"ProgressIndicator@base"> };
  /** Renders its `image` slot, or its `fallback` without one; draws nothing else. */
  export const Avatar: { new (): Element<"Avatar@base">; new: () => Element<"Avatar@base"> };
  /** The image slot of an `Avatar`: an asset or an absolute path. */
  export const AvatarImage: { new (source: string): Element<"AvatarImage@base">; new: (source: string) => Element<"AvatarImage@base"> };
  /** The fallback slot of an `Avatar`. */
  export const AvatarFallback: { new (): Element<"AvatarFallback@base">; new: () => Element<"AvatarFallback@base"> };
  /** An unstyled semantic table: nest the parts below. */
  export const Table: { new (id: string | number): Element<"Table@base">; new: (id: string | number) => Element<"Table@base"> };
  /** The header row group. */
  export const TableHeader: { new (id: string | number): Element<"TableHeader@base">; new: (id: string | number) => Element<"TableHeader@base"> };
  /** The body row group. */
  export const TableBody: { new (id: string | number): Element<"TableBody@base">; new: (id: string | number) => Element<"TableBody@base"> };
  /** One row, `row_index` one-based. */
  export const TableRow: { new (id: string | number, row_index: number): Element<"TableRow@base">; new: (id: string | number, row_index: number) => Element<"TableRow@base"> };
  /** One column header, `column_index` one-based. */
  export const TableHead: { new (id: string | number, column_index: number): Element<"TableHead@base">; new: (id: string | number, column_index: number) => Element<"TableHead@base"> };
  /** One data cell, `column_index` one-based. */
  export const TableCell: { new (id: string | number, column_index: number): Element<"TableCell@base">; new: (id: string | number, column_index: number) => Element<"TableCell@base"> };
  /** A caption slot. */
  export const TableCaption: { new (id: string | number): Element<"TableCaption@base">; new: (id: string | number) => Element<"TableCaption@base"> };
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
    html(id: string, html: string): Element<"TextView@base">;
    markdown(id: string, markdown: string): Element<"TextView@base">;
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
  const dock_state: unique symbol;
  /** A layout from `dump()`, opaque: keep it, or `JSON.stringify` it and parse it back. */
  export interface DockState {
    readonly [dock_state]: true;
  }
  /** A dockable layout the user rearranges. Make it in `init` or a handler. */
  export interface DockArea {
    /** Docks `view`, a view from `cx.new(Class)`. */
    add_panel(view: import("corona/kit").Entity, options: DockPanelOptions): void;
    remove_panel(id: number): boolean;
    panels(): DockPanel[];
    /** The layout as plain data, with each panel's `serialize()`. */
    dump(): DockState;
    /** Restores a `dump()`, rebuilding registered panels and calling their `deserialize(data)`. */
    load(state: DockState): void;
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
  export function dock_area(area: DockArea): Element<"dock_area@base">;
  /** Where a dock's panels go inside its chrome handler. */
  export const dock_content: { new (): Element<"dock_content@base">; new: () => Element<"dock_content@base"> };
}
declare module "corona/component" {
  import { ClickEvent, Context, Element, FocusHandle } from "corona/kit";
  export const Probe: { new (): Element<"Probe">; new: () => Element<"Probe"> };
  /** A styled command button. */
  export const Button: { new (id: string | number): Element<"Button">; new: (id: string | number) => Element<"Button"> };
  /** An external-resource link. */
  export const Link: { new (id: string | number): Element<"Link">; new: (id: string | number) => Element<"Link"> };
  /** A controlled check box: `checked(..)` in, `on_change(..)` out. */
  export const Checkbox: { new (id: string | number): Element<"Checkbox">; new: (id: string | number) => Element<"Checkbox"> };
  /** A controlled switch: `checked(..)` in, `on_change(..)` out. */
  export const Switch: { new (id: string | number): Element<"Switch">; new: (id: string | number) => Element<"Switch"> };
  /** One option of a radio group. */
  export const Radio: { new (id: string | number): Element<"Radio">; new: (id: string | number) => Element<"Radio"> };
  /** A column of `Radio`s; `on_change(index => ..)`. */
  export const RadioGroup: { new (id: string | number): Element<"RadioGroup">; new: (id: string | number) => Element<"RadioGroup"> };
  /** A row of `Radio`s; `on_change(index => ..)`. */
  export const HorizontalRadioGroup: { new (id: string | number): Element<"RadioGroup">; new: (id: string | number) => Element<"RadioGroup"> };
  /** A button that stays down: `checked(..)` in, `on_change(..)` out. */
  export const Toggle: { new (id: string | number): Element<"Toggle">; new: (id: string | number) => Element<"Toggle"> };
  /** A button copying `value(..)`. */
  export const Clipboard: { new (id: string | number): Element<"Clipboard">; new: (id: string | number) => Element<"Clipboard"> };
  /** Stars: `value(..)`, `max(..)`, `on_change(stars => ..)`. */
  export const Rating: { new (id: string | number): Element<"Rating">; new: (id: string | number) => Element<"Rating"> };
  /** A frame around `input(..)` and its `addon(..)`s. */
  export const InputGroup: { new (id: string | number): Element<"InputGroup">; new: (id: string | number) => Element<"InputGroup"> };
  /** An addon of an `InputGroup`, at `align(..)`. */
  export const InputGroupAddon: { new (id: string | number): Element<"InputGroupAddon">; new: (id: string | number) => Element<"InputGroupAddon"> };
  /** A compact button inside an `InputGroup`. */
  export const InputGroupButton: { new (id: string | number): Element<"InputGroupButton">; new: (id: string | number) => Element<"InputGroupButton"> };
  /** The frame around retained text. */
  export const Input: { new (state: InputState): Element<"Input">; new: (state: InputState) => Element<"Input"> };
  /** A spinbutton over an `InputState`. */
  export const NumberInput: { new (state: InputState): Element<"NumberInput">; new: (state: InputState) => Element<"NumberInput"> };
  /** The single-line control of an `InputGroup`. */
  export const InputGroupInput: { new (state: InputState): Element<"Input">; new: (state: InputState) => Element<"Input"> };
  /** The frame around retained multi-line text. */
  export const Textarea: { new (state: TextareaState): Element<"Textarea">; new: (state: TextareaState) => Element<"Textarea"> };
  /** The multi-line control of an `InputGroup`. */
  export const InputGroupTextarea: { new (state: TextareaState): Element<"Textarea">; new: (state: TextareaState) => Element<"Textarea"> };
  /** A fixed-length code. */
  export const OtpInput: { new (state: OtpState): Element<"OtpInput">; new: (state: OtpState) => Element<"OtpInput"> };
  /** A slider over a retained `SliderState`. */
  export const Slider: { new (state: SliderState): Element<"Slider">; new: (state: SliderState) => Element<"Slider"> };
  /** A color picker. */
  export const ColorPicker: { new (state: ColorPickerState): Element<"ColorPicker">; new: (state: ColorPickerState) => Element<"ColorPicker"> };
  /** A date picker. */
  export const DatePicker: { new (state: DatePickerState): Element<"DatePicker">; new: (state: DatePickerState) => Element<"DatePicker"> };
  /** A time of day field. */
  export const TimeField: { new (state: TimeFieldState): Element<"TimeField">; new: (state: TimeFieldState) => Element<"TimeField"> };
  /** A text label. */
  export const Label: { new (text: string): Element<"Label">; new: (text: string) => Element<"Label"> };
  /** A keyboard shortcut keycap, e.g. `"ctrl-s"`. */
  export const Kbd: { new (keystroke: string): Element<"Kbd">; new: (keystroke: string) => Element<"Kbd"> };
  /** A labeled form field. */
  export const Field: { new (): Element<"Field">; new: () => Element<"Field"> };
  /** A form of `Field`s. */
  export const Form: { new (): Element<"Form">; new: () => Element<"Form"> };
  /** A form laying out its labels above. */
  export const VForm: { new (): Element<"Form">; new: () => Element<"Form"> };
  /** A form laying out its labels beside. */
  export const HForm: { new (): Element<"Form">; new: () => Element<"Form"> };
  /** Muted text inside an `InputGroup`. */
  export const InputGroupText: { new (): Element<"InputGroupText">; new: () => Element<"InputGroupText"> };
  /** A native select over the `{ id, label, disabled? }` rows `rows()` returns. */
  export const Select: { new (id: string, rows: () => readonly SelectRow[], render_row: (row: SelectRow) => Element | null, on_select: (value: string, cx: Context) => void): Element<"Select">; new: (id: string, rows: () => readonly SelectRow[], render_row: (row: SelectRow) => Element | null, on_select: (value: string, cx: Context) => void) => Element<"Select"> };
  /** A single-select searchable combobox over the rows `rows()` returns. */
  export const Combobox: { new (id: string, rows: () => readonly SelectRow[], on_change: (value: string, cx: Context) => void, on_confirm: (value: string, cx: Context) => void): Element<"Combobox">; new: (id: string, rows: () => readonly SelectRow[], on_change: (value: string, cx: Context) => void, on_confirm: (value: string, cx: Context) => void) => Element<"Combobox"> };
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
  type Ctor<K extends string, A extends any[]> = { new (...args: A): Element<K>; new: (...args: A) => Element<K> };
  /** A styled tab bar taking `Tab` children. */
  export const TabBar: Ctor<"TabBar", [id: string | number]>;
  /** @deprecated Use `TabBar`. */
  export const Tabs: Ctor<"TabBar", [id: string | number]>;
  /** One tab of a `TabBar`. */
  export const Tab: Ctor<"Tab", []>;
  /** An accordion taking `AccordionItem` children. */
  export const Accordion: Ctor<"Accordion", [id: string | number]>;
  /** One item: `title(..)`, `open(..)`, children as its content. */
  export const AccordionItem: Ctor<"AccordionItem", []>;
  /** Renders its `content` only while `open`. */
  export const Collapsible: Ctor<"Collapsible", []>;
  /** Page buttons: `current_page`, `total_pages`, `on_change(page => ..)`. */
  export const Pagination: Ctor<"Pagination", [id: string | number]>;
  /** Steps taking `StepperItem` children. */
  export const Stepper: Ctor<"Stepper", [id: string | number]>;
  export const StepperItem: Ctor<"StepperItem", []>;
  /** A trail of labels. */
  export const Breadcrumb: Ctor<"Breadcrumb", [labels: string[]]>;
  /** A titled container. */
  export const GroupBox: Ctor<"GroupBox", []>;
  /** A bar: children in the middle, `left_content`/`right_content` at the edges. */
  export const StatusBar: Ctor<"StatusBar", []>;
  export const Toolbar: Ctor<"Toolbar", [id: string | number]>;
  export const Separator: Ctor<"Separator", []>;
  export const VerticalSeparator: Ctor<"Separator", []>;
  export const DashedSeparator: Ctor<"Separator", []>;
  export const VerticalDashedSeparator: Ctor<"Separator", []>;
  /** Panes with draggable dividers, horizontal unless `axis("vertical")`; takes `ResizablePanel`s. */
  export const Resizable: Ctor<"Resizable", [id: string | number]>;
  export const ResizablePanel: Ctor<"ResizablePanel", []>;
  /** A vertically scrolling area tracking `handle`; `scroll_axis(..)` changes the axes. */
  export const Scroll: Ctor<"Scroll", [handle: ScrollbarHandle]>;
  /** A scrollbar driving the area that tracks `handle`. */
  export const Scrollbar: Ctor<"Scrollbar", [id: string | number, handle: string | number | ScrollbarHandle | import("corona/base").VirtualListScrollHandle]>;
  export type ScrollbarHandle = import("corona/kit").ScrollHandle;
  export const ScrollbarHandle: { (): ScrollbarHandle; new: () => ScrollbarHandle };
  /** A collapsible sidebar taking `SidebarMenu` children, `header(..)` and `footer(..)`. */
  export const Sidebar: Ctor<"Sidebar", [id: string | number]>;
  export const SidebarHeader: Ctor<"SidebarHeader", []>;
  export const SidebarFooter: Ctor<"SidebarFooter", []>;
  /** Rows: `SidebarMenuItem` children. */
  export const SidebarMenu: Ctor<"SidebarMenu", []>;
  /** A row; `SidebarMenuItem` children make its submenu. */
  export const SidebarMenuItem: Ctor<"SidebarMenuItem", [label: string]>;
  export const SidebarToggleButton: Ctor<"SidebarToggleButton", []>;
  /** A button opening a menu of `item(label, cx => ..)`s. */
  export const DropdownMenu: Ctor<"DropdownMenu", [id: string | number, label: string]>;
  /** A button popping up the OS menu of its `NativeMenuItem` children. */
  export const NativeMenuTrigger: Ctor<"NativeMenuTrigger", [id: string | number, label: string]>;
  /** An item dispatching `action`, for `on_action`. */
  export const NativeMenuItem: Ctor<"NativeMenuItem", [label: string, action: string]>;
  export const NativeMenuSeparator: Ctor<"NativeMenuSeparator", []>;
  /** A command palette's query and selection; make it in `init`. */
  export interface CommandState {
    /** The search text. */
    query(): string;
    release(): boolean;
  }
  export const CommandState: { (): CommandState; new: () => CommandState };
  /** A searchable command palette taking `CommandItem`, `CommandGroup` and `CommandSeparator`. */
  export const Command: Ctor<"Command", [state: CommandState]>;
  export const CommandItem: Ctor<"CommandItem", [label: string]>;
  /** A labelled section taking `CommandItem` children. */
  export const CommandGroup: Ctor<"CommandGroup", [label: string]>;
  export const CommandSeparator: Ctor<"CommandSeparator", []>;
  /** An application menu of `MenuItem`s and `MenuSeparator`s, for a `MenuBar`. */
  export const Menu: Ctor<"Menu", [label: string]>;
  /** A menu item dispatching `action`, for `on_action`. */
  export const MenuItem: Ctor<"MenuItem", [label: string, action: string]>;
  export const MenuSeparator: Ctor<"MenuSeparator", []>;
  /** Installs its `Menu` children as the application menus and draws them in the window. */
  export const MenuBar: Ctor<"MenuBar", [id: string]>;
  /** A labelled button with a menu of `menu_item(label, cx => ..)`s. */
  export const DropdownButton: Ctor<"DropdownButton", [id: string | number, label: string]>;
  /** A ghost button labelled `label` opening a styled surface: `content(..)`, `open(..)`, `on_open_change(..)`. */
  export const Popover: { new (id: string, label: string): Element<"Popover">; new: (id: string, label: string) => Element<"Popover"> };
  /** A styled surface opened by hovering `trigger_element(..)`. */
  export const HoverCard: { new (id: string | number): Element<"HoverCard">; new: (id: string | number) => Element<"HoverCard"> };
  /** A button `label` showing `text` on hover. */
  export const Tooltip: { new (id: string, label: string, text: string): Element<"Button">; new: (id: string, label: string, text: string) => Element<"Button"> };
  /** A button `label` opening a dialog of `title(..)` and `content(() => ..)`. */
  export const Dialog: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element<"Dialog">; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element<"Dialog"> };
  /** A button `label` opening an alert of `title(..)` and `description(..)`. */
  export const AlertDialog: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element<"AlertDialog">; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element<"AlertDialog"> };
  /** A button `label` opening a sheet of `content(() => ..)` at `placement(..)`. */
  export const Sheet: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element<"Sheet">; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element<"Sheet"> };
  /** A button `label` opening a toast of `title(..)` and `message(..)`. */
  export const Notification: { new (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void): Element<"Notification">; new: (id: string, label: string, on_effect_error?: (message: string, cx: Context) => void) => Element<"Notification"> };
  /** An inline message. */
  export const Alert: { new (id: string | number, message: string): Element<"Alert">; new: (id: string | number, message: string) => Element<"Alert"> };
  /** An inline message. */
  export const InfoAlert: { new (id: string | number, message: string): Element<"Alert">; new: (id: string | number, message: string) => Element<"Alert"> };
  /** An inline message. */
  export const SuccessAlert: { new (id: string | number, message: string): Element<"Alert">; new: (id: string | number, message: string) => Element<"Alert"> };
  /** An inline message. */
  export const WarningAlert: { new (id: string | number, message: string): Element<"Alert">; new: (id: string | number, message: string) => Element<"Alert"> };
  /** An inline message. */
  export const ErrorAlert: { new (id: string | number, message: string): Element<"Alert">; new: (id: string | number, message: string) => Element<"Alert"> };
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
  export const Badge: { new (): Element<"Badge">; new: () => Element<"Badge"> };
  /** A small label. */
  export const Tag: { new (): Element<"Tag">; new: () => Element<"Tag"> };
  /** A loading placeholder; size it. */
  export const Skeleton: { new (): Element<"Skeleton">; new: () => Element<"Skeleton"> };
  /** A turning loading icon. */
  export const Spinner: { new (): Element<"Spinner">; new: () => Element<"Spinner"> };
  /** An empty state: `header`, `content` and children. */
  export const Empty: { new (): Element<"Empty">; new: () => Element<"Empty"> };
  /** The header of an `Empty`: `media`, `title`, `description`. */
  export const EmptyHeader: { new (): Element<"EmptyHeader">; new: () => Element<"EmptyHeader"> };
  /** The media of an `EmptyHeader`. */
  export const EmptyMedia: { new (): Element<"EmptyMedia">; new: () => Element<"EmptyMedia"> };
  /** The title of an `EmptyHeader`. */
  export const EmptyTitle: { new (): Element<"EmptyTitle">; new: () => Element<"EmptyTitle"> };
  /** The description of an `EmptyHeader`. */
  export const EmptyDescription: { new (): Element<"EmptyDescription">; new: () => Element<"EmptyDescription"> };
  /** The actions of an `Empty`. */
  export const EmptyContent: { new (): Element<"EmptyContent">; new: () => Element<"EmptyContent"> };
  /** An icon from the app's assets. */
  export const Icon: { new (path: string): Element<"Icon">; new: (path: string) => Element<"Icon"> };
  /** An image from the app's assets, or a file by absolute path. */
  export const Image: { new (path: string): Element<"image">; new: (path: string) => Element<"image"> };
  /** Labelled values: add `DescriptionItem`s with `items`. */
  export const DescriptionList: { new (): Element<"DescriptionList">; new: () => Element<"DescriptionList"> };
  /** One labelled value of a `DescriptionList`. */
  export const DescriptionItem: { new (label: string): Element<"DescriptionItem">; new: (label: string) => Element<"DescriptionItem"> };
  /** A progress bar, `value` 0–100. */
  export const Progress: { new (id: string | number): Element<"Progress">; new: (id: string | number) => Element<"Progress"> };
  /** A round picture, or the initials of `name`. */
  export const Avatar: { new (): Element<"Avatar">; new: () => Element<"Avatar"> };
  /** Bars of `{ label, value }` rows, or a function returning them; fills its size. */
  export const BarChart: { new (rows: ChartRows): Element<"BarChart">; new: (rows: ChartRows) => Element<"BarChart"> };
  /** A line through the rows. */
  export const LineChart: { new (rows: ChartRows): Element<"LineChart">; new: (rows: ChartRows) => Element<"LineChart"> };
  /** A filled line through the rows. */
  export const AreaChart: { new (rows: ChartRows): Element<"AreaChart">; new: (rows: ChartRows) => Element<"AreaChart"> };
  /** Slices of the rows. */
  export const PieChart: { new (rows: ChartRows): Element<"PieChart">; new: (rows: ChartRows) => Element<"PieChart"> };
  /** A radar of the rows. */
  export const RadarChart: { new (rows: ChartRows): Element<"RadarChart">; new: (rows: ChartRows) => Element<"RadarChart"> };
  /** A month grid of a `CalendarState`, picking dates. */
  export const Calendar: { new (state: CalendarState): Element<"Calendar">; new: (state: CalendarState) => Element<"Calendar"> };
  /** Slides: `CarouselContent` with `CarouselItem`s, and the controls. */
  export const Carousel: { new (id: string | number, state: CarouselState): Element<"Carousel">; new: (id: string | number, state: CarouselState) => Element<"Carousel"> };
  /** The clipped viewport of a `Carousel`. */
  export const CarouselContent: { new (state: CarouselState): Element<"CarouselContent">; new: (state: CarouselState) => Element<"CarouselContent"> };
  /** Slide `index`. */
  export const CarouselItem: { new (id: string | number, index: number, state: CarouselState): Element<"CarouselItem">; new: (id: string | number, index: number, state: CarouselState) => Element<"CarouselItem"> };
  /** The previous-slide button. */
  export const CarouselPrevious: { new (state: CarouselState): Element<"CarouselPrevious">; new: (state: CarouselState) => Element<"CarouselPrevious"> };
  /** The next-slide button. */
  export const CarouselNext: { new (state: CarouselState): Element<"CarouselNext">; new: (state: CarouselState) => Element<"CarouselNext"> };
  /** The row of `CarouselPaginationItem`s. */
  export const CarouselPagination: { new (): Element<"CarouselPagination">; new: () => Element<"CarouselPagination"> };
  /** The dot of slide `index`. */
  export const CarouselPaginationItem: { new (id: string | number, index: number, state: CarouselState): Element<"CarouselPaginationItem">; new: (id: string | number, index: number, state: CarouselState) => Element<"CarouselPaginationItem"> };
  /** A list of rows rendered lazily by `render_row`; its state is kept by `id`. */
  export const List: { new <T>(id: string, rows: readonly T[] | (() => readonly T[]), render_row: (row: T, cx: Context) => Element): Element; new: <T>(id: string, rows: readonly T[] | (() => readonly T[]), render_row: (row: T, cx: Context) => Element) => Element<"List"> };
  /** A table of rows, its visible cells rendered lazily by `render_cell`. */
  export const DataTable: { new <T>(state: DataTableState, rows: readonly T[] | (() => readonly T[]), render_cell: (row: T, column: string, cx: Context) => Element): Element; new: <T>(state: DataTableState, rows: readonly T[] | (() => readonly T[]), render_cell: (row: T, column: string, cx: Context) => Element) => Element<"DataTable"> };
  /** A tree of `TreeItem` children; what is expanded is kept by `id`. */
  export const Tree: { new (id: string): Element<"Tree">; new: (id: string) => Element<"Tree"> };
  /** One node of a `Tree`; nest more as children. */
  export const TreeItem: { new (id: string, label: string): Element<"TreeItem">; new: (id: string, label: string) => Element<"TreeItem"> };
  /** A styled table of `TableHeader`, `TableBody`, `TableFooter` and `TableCaption`. */
  export const Table: { new (): Element<"Table">; new: () => Element<"Table"> };
  /** The header rows of a `Table`. */
  export const TableHeader: { new (): Element<"TableHeader">; new: () => Element<"TableHeader"> };
  /** The body rows of a `Table`. */
  export const TableBody: { new (): Element<"TableBody">; new: () => Element<"TableBody"> };
  /** The footer rows of a `Table`. */
  export const TableFooter: { new (): Element<"TableFooter">; new: () => Element<"TableFooter"> };
  /** A row of `TableHead`s or `TableCell`s. */
  export const TableRow: { new (): Element<"TableRow">; new: () => Element<"TableRow"> };
  /** A column header cell. */
  export const TableHead: { new (): Element<"TableHead">; new: () => Element<"TableHead"> };
  /** A data cell. */
  export const TableCell: { new (): Element<"TableCell">; new: () => Element<"TableCell"> };
  /** A caption under a `Table`. */
  export const TableCaption: { new (): Element<"TableCaption">; new: () => Element<"TableCaption"> };
  export type ComponentSize = "xsmall" | "small" | "medium" | "large";
  /** A file or image attachment; its children fill its content. */
  export const Attachment: { new (id: string): Element<"Attachment">; new: (id: string) => Element<"Attachment"> };
  /** A message bubble; its children are its content. */
  export const Bubble: { new (): Element<"Bubble">; new: () => Element<"Bubble"> };
  /** A compact conversation status marker. */
  export const Marker: { new (id: string): Element<"Marker">; new: (id: string) => Element<"Marker"> };
  /** A message row; its children fill its content. */
  export const Message: { new (): Element<"Message">; new: () => Element<"Message"> };
  /** Animated loading text. */
  export const ShimmerText: { new (text: string): Element<"ShimmerText">; new: (text: string) => Element<"ShimmerText"> };
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
    new (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null): Element<"MessageScroller">;
    new: (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null) => Element<"MessageScroller">;
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
  export const Editor: { new (state: EditorState): Element<"Editor">; new: (state: EditorState) => Element<"Editor"> };
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
  export const Diff: { new (state: DiffState): Element<"Diff">; new: (state: DiffState) => Element<"Diff"> };
  /** Plain text in a styleable wrapper. */
  export const Text: { new (value: string): Element<"div">; new: (value: string) => Element<"div"> };
  /** A settings surface; takes `SettingPage` children. */
  export const Settings: { new (id: string): Element<"Settings">; new: (id: string) => Element<"Settings"> };
  /** A settings page; takes `SettingGroup` children. */
  export const SettingPage: { new (title: string): Element<"SettingPage">; new: (title: string) => Element<"SettingPage"> };
  /** A settings group; takes `SettingItem` children. */
  export const SettingGroup: { new (): Element<"SettingGroup">; new: () => Element<"SettingGroup"> };
  /** One setting; `content(() => field)` renders its field. */
  export const SettingItem: { new (title: string): Element<"SettingItem">; new: (title: string) => Element<"SettingItem"> };
  /** A questionnaire keyed by `id`; takes `QuestionnaireItem` children, rebuilt when they change. */
  export const Questionnaire: { new (id: string): Element<"Questionnaire">; new: (id: string) => Element<"Questionnaire"> };
  /** One question; takes `QuestionnaireChoice` children. */
  export const QuestionnaireItem: { new (name: string, label: string): Element<"QuestionnaireItem">; new: (name: string, label: string) => Element<"QuestionnaireItem"> };
  /** A free-form answer of a question, typed into `state`. */
  export const QuestionnaireInput: { new (state: InputState, label: string): Element<"QuestionnaireInput">; new: (state: InputState, label: string) => Element<"QuestionnaireInput"> };
  /** One answer of a question. */
  export const QuestionnaireChoice: { new (value: string, label: string): Element<"QuestionnaireChoice">; new: (value: string, label: string) => Element<"QuestionnaireChoice"> };
}
declare module "corona/fps" {
  import { Element } from "corona/kit";
  export type Anchor = "top_left" | "top_right" | "bottom_left" | "bottom_right" | "top_center" | "bottom_center" | "left_center" | "right_center";
  /** The performance HUD as an element; its parent must be `relative()`. */
  export function fps_monitor(): Element<"fps_monitor">;
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
