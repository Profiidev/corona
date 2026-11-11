# Embedding JS widgets in Rust: QuickJS builder style + typed host API

Oct 23, 2026

## Summary

Widgets will run on **QuickJS via rquickjs**, build UI with a **gpui-style builder API** (`div().flex().child(...)`), and get **TypeScript typings generated from Rust**. Widgets render only when their state changes, so QuickJS's speed gap to V8 (about 0.7 ms vs 0.06 ms for 1,000 rows) costs little. Its footprint is 6–10× smaller in memory and 25× smaller on disk.

| Decision                      | Choice                                               | Main reason                                                                |
| ----------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------- |
| JS engine                     | QuickJS (`rquickjs` 0.14)                            | 4.4 MiB peak RSS, 0.37 ms startup, 1.8 MiB binary, best binding ergonomics |
| UI API style                  | Host builder methods                                 | Mirrors gpui 1:1, typos throw immediately, no per-render garbage           |
| Builder method typings        | Own `macro_rules!` table emitting bindings + `.d.ts` | One line per method yields both the binding and its type                   |
| Program host function typings | `#[specta]` + `rquickjs::Func::from`                 | Binding and types both derived from the plain Rust fn signature            |
| `&mut App` access             | Raw-pointer scope set by an `Enter` guard per call   | No runtime allows borrowing non-`'static` state safely                     |

## Context

The app embeds JS widgets in a Rust gpui program. Each widget is a JS file that default-exports a class extending `Widget` with `init(cx)` and `render(cx)`. Host functions mutate the global `&mut App` and a `Window<'a>` that has a non-`'static` lifetime. `render()` runs at most once per frame, and only when the widget's state changed.

```js
import { Widget, div, text } from 'gpui';

export default class Counter extends Widget {
  init(cx) {
    cx.set(0, 0);
  }
  render(cx) {
    return div()
      .flex()
      .gap(4)
      .bg(0x202020)
      .child(text('Count: ' + cx.get(0)))
      .child(
        div()
          .px(8)
          .bg(0x3060ff)
          .child(text('+'))
          .onClick((cx) => cx.set(0, cx.get(0) + 1))
      );
  }
}
```

The benchmark repo (`rustjs`) implemented this widget API five times against a mock gpui:

- **V8:** the `v8` crate 150.4.0, with fast API calls.
- **deno_core:** 0.413, with `#[op2(fast)]` ops.
- **QuickJS:** `rquickjs` 0.14.
- **JavaScriptCore (GObject API):** `javascriptcore-rs` 2.
- **JavaScriptCore (C API):** a hand-written FFI to the raw C API.

The same JS sources ran on every runtime, and a hash of the resulting frames and state confirmed that all runtimes did identical work.

| Workload | One iteration                                         |
| -------- | ----------------------------------------------------- |
| counter  | Render a 4-node tree (per-call overhead)              |
| list     | Render 1,000 styled rows (2,001 nodes)                |
| state    | 10,000 `cx.get`/`cx.set` host calls into `&mut App`   |
| events   | Fire 100 stored JS `onClick` closures, then re-render |

Measured on Linux x86_64 in the default release profile; timings are the best-of-3 median.

## Why QuickJS

QuickJS is 10–25× slower than V8 per render, but a 1,000-row render still takes only 8.6% of a 120 Hz frame (0.72 of 8.3 ms). It wins every cost that applies on every launch: memory, startup, binary size and binding effort.

| Runtime       | 1k-row list (µs) | 10k host calls (µs) | Peak RSS (MiB) | Startup (ms) | Stripped binary (MiB)   | Clean release build (s) |
| ------------- | ---------------- | ------------------- | -------------- | ------------ | ----------------------- | ----------------------- |
| QuickJS       | 716              | 871                 | 4.4            | 0.37         | 1.8                     | 22.1                    |
| V8 (fast API) | 58               | 32                  | 27.7           | 3.1          | 44.4                    | 8.7                     |
| deno_core     | 68               | 55                  | 32.6           | 9.3          | 48.4                    | 18.0                    |
| JSC C API     | 1,714            | 2,539               | 61.5           | 5.5          | 0.4 + 40 MiB system lib | 0.9                     |
| JSC GObject   | 4,319            | 5,017               | 62.1           | 5.5          | 0.4 + 40 MiB system lib | 7.6                     |

Render times are for the builder style. V8's build is fast only because it links a prebuilt 37 MB static library; building V8 from source takes about an hour.

**Why the speed gap matters little here:**

- No state change means no JS call at all, so idle cost is zero on every engine.
- A click handler costs about 1 µs in QuickJS.
- The benchmark called `render()` thousands of times in a row, so V8 ran fully optimized. Called once per state change, V8 stays in its interpreter or baseline compiler and fast API calls never engage, so the real gap is smaller than measured.

**When to revisit:** long lists without virtualization (10k rows is about 7 ms in QuickJS), many widgets re-rendering in the same frame, per-frame animation driven from JS, or heavy non-UI computation in JS. In those cases deno_core gives V8 speed with little binding code.

**Why not JSC:** its public API has no ES module support, so widget sources must be rewritten into scripts. Every C API callback takes and releases `JSLock`, which is about 45% of host-call time in a perf profile. The safe `javascriptcore-rs` crate cannot define host functions or classes at all.

## Why builder style

The builder style is chosen for its ergonomics, not its speed. On QuickJS, returning a plain object tree was 20% faster (574 vs 716 µs per 1,000 rows), a difference of about 0.14 ms per render.

|                    | Host builder methods (chosen)                      | Returned object tree                           |
| ------------------ | -------------------------------------------------- | ---------------------------------------------- |
| Syntax             | `div().flex().child(..)`, same as gpui in Rust     | `{ style, children, onClick }`, React/JSX-like |
| Typo such as `bgg` | Throws a TypeError at runtime, error in the editor | Silently ignored unless Rust validates keys    |
| Garbage per render | Almost none, nodes live in the Rust arena          | The whole tree, every render                   |
| Binding work       | One host method per style method                   | One tree walker                                |
| QuickJS, 1k rows   | 716 µs                                             | 574 µs                                         |

Why the tree is faster on QuickJS: each builder call crosses into Rust at about 87 ns, roughly 5 calls per row. Object literals are cheap in the interpreter, and the Rust walker reads them with pre-interned atoms. On V8 it is the other way round (58 vs 240 µs), because fast API calls make host calls about 3 ns.

The binding work for gpui's hundreds of style methods is handled by the typing macro, which generates every method from one table.

## Passing `&mut App` and `Window<'a>`

Host functions reach `&mut App` and `Window<'a>` through raw pointers that a guard sets around each call into JS. No engine can let a stored host callback borrow non-`'static` state, because JS may call it at any time. rquickjs's `Ctx::store_userdata` is a TypeId map that needs `JsLifetime` types, and `#[methods]` cannot capture anything, so the scope lives in a thread-local.

```rust
struct Scope { app: *mut App, frame: *mut Frame, handlers: Vec<Persistent<Function<'static>>> }
thread_local! { static SCOPE: UnsafeCell<Scope> = /* null pointers */; }

/// Sets the scope for one call into JS; nulls it on drop, including on panic.
struct Enter;
impl Enter {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let s = scope();
        s.app = cx;
        s.frame = &mut *window.frame;
        Enter
    }
}
impl Drop for Enter { fn drop(&mut self) { /* app = frame = null */ } }

fn app() -> &'static mut App {
    let p = scope().app;
    assert!(!p.is_null(), "host fn called outside render/init/dispatch");
    unsafe { &mut *p }
}
```

The unsafety stays in three places: the guard and two accessors that assert the pointer is set. A host function called outside `init`, `render` or an event dispatch panics instead of touching freed memory.

One trap: `Persistent` handles must be dropped before the `rquickjs::Runtime`, otherwise `JS_FreeRuntime` aborts on an assertion. Declare the runtime fields last in the struct and clear the handler list in `Drop`.

## Generating TypeScript typings

Typings come from two sources: an own macro table for builder methods, and the specta crate for every other host function. None of the five runtimes generates `.d.ts` files itself; rquickjs, deno_core's `#[op2]`, v8 and JSC all lack it. Both outputs go into one `gpui.d.ts`, which a test regenerates.

| API surface                    | Mechanism                  | Binding comes from                      | Types come from               | Status                      |
| ------------------------------ | -------------------------- | --------------------------------------- | ----------------------------- | --------------------------- |
| Builder methods (`div().bg()`) | Own `macro_rules!` table   | Macro emits `#[rquickjs::methods]` impl | Same macro emits TS text      | Sketch, not compiled        |
| Program host functions         | `#[specta]` + `Func::from` | rquickjs, from the fn signature         | specta, from the fn signature | Prototype compiles and runs |

### Builder methods: one macro table

Each method is one line in a table. The macro expands it into the rquickjs method and the matching TS signature, copying doc comments as JSDoc.

```rust
macro_rules! ts_ty {
    (i32) => { "number" }; (u32) => { "number" }; (bool) => { "boolean" };
    (String) => { "string" }; (El) => { "El" }; (Handler) => { "(cx: Cx) => void" };
}

gpui_el! {
    /// Lay children out in a row.
    fn flex() { |id, f: &mut Frame| f.style(id).flex = true }
    /// Background color as 0xRRGGBB.
    fn bg(color: u32) { |id, f: &mut Frame| f.style(id).bg = color }
    fn child(el: El) { |id, f: &mut Frame| f.child(id, el.borrow().id) }
    fn onClick(handler: Handler) { |id, f: &mut Frame| register_handler(id, f, handler) }
}
// expands to: #[rquickjs::methods] impl El { fn flex<'js>(this: This<Class<'js, El>>) -> Class<'js, El> { .. } .. }
//           + pub fn el_d_ts() -> String
```

Rust method names must equal the JS names (`onClick`), so the macro allows `non_snake_case`. specta cannot cover these methods: their `This<Class<El>>` receiver is an rquickjs type, and the orphan rule forbids implementing specta's trait for it.

### Program host functions: specta + rquickjs

Host functions that call into the rest of the program are plain Rust fns. rquickjs's `Func::from` derives the JS binding from the signature through `FromJs` and `IntoJs`. specta's `#[specta]` records the same signature, and `specta-typescript` prints it. Verified in a prototype:

```rust
/// Read a file and bump the counter.
#[specta]
fn read_file(cx: Cx, path: String, opts: Opts) -> FileInfo {
    cx.0.count += 1; // &mut App, injected
    FileInfo { size: path.len() as u32, path }
}

/// Parse a number or throw.
#[specta]
fn parse(s: String) -> Throws<i32> { s.trim().parse::<i32>().into() }

#[derive(Type, IntoJs)] pub struct FileInfo { pub path: String, pub size: u32 }
#[derive(Type, FromJs)] pub struct Opts { pub follow: bool }

globals.set("read_file", Func::from(read_file))?;
```

Generated output, as printed by the prototype:

```ts
/**  A file read from disk. */
export type FileInfo = { path: string; size: number };
export type Opts = { follow: boolean };
export function read_file(path: string, opts: Opts): FileInfo;
export function count(): number;
```

Called from JS, `read_file('a.txt', { follow: true })` returns the object and updates `App`; `parse('x')` throws an `Error` with the message "invalid digit found in string".

Two pieces of glue make this work, about 30 lines in total:

- **`Cx` injection.** `Cx` implements rquickjs's `FromParam` with `ParamRequirement::none()`, so it is filled from the scope pointer without consuming a JS argument. It also implements specta's `FunctionArg` returning `None`, so it is left out of the TS signature. Add one such type per program part, such as `Db` or `Window`.
- **`Throws<T>`.** specta renders `Result<T, E>` as a union instead of "returns T or throws", and `rquickjs::Error` does not implement specta's `Type`. `Throws<T>` implements `IntoJs` (an `Err` becomes `Exception::throw_message`), specta's `FunctionResult` (TS sees only `T`), and `From<Result<T, E: Display>>`. `?` cannot return a `Throws` directly, so fallible bodies use a closure that returns a `Result`, then call `.into()`.

All host functions are listed once in a registration macro, `host_module!(read_file, parse, count)`. It generates `install()`, which calls `Func::from` for each fn and can feed a native `ModuleDef` for `import { read_file } from "app"`, and `d_ts()`, which runs `fn_datatype!` on each fn and renders it with `primitives::reference`. This macro is a sketch and has not been compiled.

## Risks and next steps

The largest risk is specta's release-candidate API; the rest are known limits with workarounds.

| Risk                      | Impact                                                    | Mitigation                                                                                                                           |
| ------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| specta is 2.0.0-rc.27     | API changes between RCs                                   | Pin `specta = "=2.0.0-rc.27"` and `specta-typescript = "=0.0.14"`                                                                    |
| `u64`/`i64` in signatures | TS exporter refuses them by default (would need `bigint`) | Use `u32`/`f64`, or configure the exporter                                                                                           |
| Async host functions      | Not tested                                                | specta records the future's output and asyncness, so emit `Promise<T>`; rquickjs needs its `Async` wrapper and the `futures` feature |
| Large unvirtualized lists | 10k rows is about 7 ms in QuickJS                         | Use gpui's `uniform_list`/`list` so only visible rows render                                                                         |
| Raw-pointer scope         | Unsound if a pointer outlives its call                    | Only `Enter` sets pointers; accessors assert non-null                                                                                |

Next steps:

- [ ] Compile the `gpui_el!` builder macro and the `host_module!` registration macro in `rt-quickjs`
- [ ] Add a test that regenerates `gpui.d.ts` and fails CI when it changes
- [ ] Add a cold-render benchmark (first renders, no warm-up) to measure the real V8 gap
- [ ] Generate the full gpui style-method table from gpui's `Styled` trait
