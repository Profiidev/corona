//! The typings of the element tree: every method is typed on the kinds it works on, every
//! constructor makes its kind, and the capability lists, kinds and methods match what the
//! runtime does.

use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::{self as gpui, TestAppContext};

use super::{eval, rooted};
use crate::element::{
  caps::{INTERACTIVE, PARENT, STYLE, caps},
  methods::ROLES,
  style::NULLARY,
};

/// The generated typings, checked against the code by `d_ts_unchanged`
const D_TS: &str = include_str!("kit.d.ts");

/// Methods on every element, whatever its kind
const ANY_KIND: &[&str] = &["map", "when", "when_some"];

/// The members of `interface Element`: `(name, signature line)`
fn members() -> Vec<(String, String)> {
  let start = D_TS.find("export interface Element<K").expect("Element");
  let body = &D_TS[start..];
  let body = &body[..body.find("\n  }\n").expect("end of Element")];
  body
    .lines()
    .skip(1)
    .filter(|l| l.starts_with("    ") && l[4..].starts_with(|c: char| c.is_ascii_lowercase()))
    .map(|l| {
      let sig = l.trim();
      let name: String = sig
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
      (name, sig.to_string())
    })
    .filter(|(name, sig)| sig[name.len()..].starts_with(['(', '<']))
    .collect()
}

/// The quoted kinds of the `Element<..>` types, and of the `Ctor<"Kind", ..>`, in `text`
fn kinds_in(text: &str) -> BTreeSet<String> {
  let mut kinds = BTreeSet::new();
  for open in ["Element<", "Ctor<"] {
    let mut rest = text;
    while let Some(at) = rest.find(open) {
      rest = &rest[at + open.len()..];
      let end = rest.find(['>', ',']).unwrap_or(rest.len());
      for quoted in rest[..end].split('|') {
        if let Some(kind) = quoted
          .trim()
          .strip_prefix('"')
          .and_then(|k| k.strip_suffix('"'))
        {
          kinds.insert(kind.to_string());
        }
      }
    }
  }
  kinds
}

/// The kinds of the `this: Element<..>` of a signature, none if it has no such parameter
fn this_kinds(sig: &str) -> Option<BTreeSet<String>> {
  let at = sig.find("this: Element<")?;
  let rest = &sig[at + "this: ".len()..];
  let inner = &rest[..rest
    .find(">,")
    .or_else(|| rest.find(">)"))
    .map_or(rest.len(), |i| i + 1)];
  Some(match inner {
    i if i.contains("StyledKind") => kinds_with(STYLE),
    i if i.contains("InteractiveKind") => kinds_with(INTERACTIVE),
    i if i.contains("ParentKind") => kinds_with(PARENT),
    i => kinds_in(i),
  })
}

/// The kinds taking `group`
fn kinds_with(group: u8) -> BTreeSet<String> {
  kinds_in(&format!("Element<{}>", kind_list(group)))
}

/// `"A" | "B"` of the kinds taking `group`, from the d.ts's own alias
fn kind_list(group: u8) -> String {
  let alias = match group {
    STYLE => "StyledKind",
    INTERACTIVE => "InteractiveKind",
    _ => "ParentKind",
  };
  let from = D_TS
    .find(&format!("export type {alias} = "))
    .expect("alias")
    + alias.len()
    + 15;
  D_TS[from..from + D_TS[from..].find(';').unwrap()].to_string()
}

#[test]
fn every_element_method_is_typed_on_kinds() {
  for (name, sig) in members() {
    if ANY_KIND.contains(&name.as_str()) {
      continue;
    }
    assert!(
      sig.contains("(this: Element<"),
      "{name} is callable on any element: {sig}"
    );
  }
}

#[test]
fn elements_have_no_loose_types() {
  for (name, sig) in members() {
    for loose in [" any", "any[]", "<any>", "unknown =>"] {
      assert!(!sig.contains(loose), "{name} has a loose type: {sig}");
    }
  }
}

#[test]
fn every_typed_kind_is_known() {
  for kind in kinds_in(D_TS) {
    assert!(
      caps(&kind).is_some(),
      "{kind} is typed but has no line in element/caps.rs"
    );
  }
  for (name, sig) in members() {
    for kind in this_kinds(&sig).unwrap_or_default() {
      assert!(
        caps(&kind).is_some(),
        "{name} names {kind}, which has no line in caps.rs"
      );
    }
  }
}

#[test]
fn constructors_make_their_kind() {
  for line in D_TS.lines().filter(|l| l.starts_with("  export const ")) {
    if line.contains("new (") || line.contains("Ctor<") {
      assert!(
        line.contains("Element<\"") || line.contains("Ctor<\"") || line.contains("State"),
        "a constructor makes a plain Element: {line}"
      );
    }
  }
  for line in D_TS.lines().filter(|l| l.starts_with("  export function ")) {
    assert!(
      !line.trim_end().ends_with("): Element;"),
      "a function makes a plain Element: {line}"
    );
  }
}

#[test]
fn styles_keep_their_name_on_every_styled_element() {
  let members = members();
  for (style, _) in NULLARY {
    if members
      .iter()
      .any(|(n, sig)| n == style && sig.contains("(this: Element<") && !sig.contains("StyledKind"))
    {
      assert!(
        members
          .iter()
          .any(|(n, s)| n == style && s.starts_with(&format!("{style}(this: Element<StyledKind>)"))),
        "{style} is a component method and a style, but not typed as a style"
      );
    }
  }
}

#[test]
fn roles_are_the_ones_the_runtime_knows() {
  let (_, sig) = members()
    .into_iter()
    .find(|(n, _)| n == "role")
    .expect("role");
  let typed: BTreeSet<_> = sig
    .split('"')
    .skip(1)
    .step_by(2)
    .map(String::from)
    .collect();
  let known: BTreeSet<_> = ROLES.iter().map(|(n, _)| n.to_string()).collect();
  assert_eq!(typed, known);
}

/// The sources declaring `node!(Type, "Kind", style, interactive, parent)`
const SOURCES: &[&str] = &[
  include_str!("../element/mod.rs"),
  include_str!("../element/background.rs"),
  include_str!("../element/components/data.rs"),
  include_str!("../element/components/dock.rs"),
  include_str!("../element/components/forms.rs"),
  include_str!("../element/components/misc.rs"),
  include_str!("../element/components/navigation.rs"),
  include_str!("../element/components/overlays.rs"),
];

/// `(kind, flags)` of every `node!(..)` call in `source`
fn declared(source: &str) -> Vec<(String, u8)> {
  let code: String = source
    .lines()
    .filter(|l| !l.trim_start().starts_with("//"))
    .collect::<Vec<_>>()
    .join("\n");
  let mut out = vec![];
  let mut rest = code.as_str();
  while let Some(at) = rest.find("node!(") {
    rest = &rest[at + 6..];
    if rest.starts_with('@') || rest.starts_with('$') {
      continue;
    }
    let end = rest.find(')').unwrap_or(rest.len());
    let args: Vec<String> = {
      let (mut depth, mut cur, mut args) = (0, String::new(), vec![]);
      for c in rest[..end].chars() {
        match c {
          '<' => depth += 1,
          '>' => depth -= 1,
          ',' if depth == 0 => args.push(std::mem::take(&mut cur)),
          c if !c.is_whitespace() => cur.push(c),
          _ => {}
        }
      }
      args.push(cur);
      args
    };
    let Some(kind) = args
      .get(1)
      .and_then(|k| k.strip_prefix('"')?.strip_suffix('"'))
    else {
      continue;
    };
    // a `corona/base` widget is told by its path, `base::Button` or `b::Tab`
    let kind = match args[0].starts_with("base::") || args[0].starts_with("b::") {
      true => format!("{kind}@base"),
      false => kind.to_string(),
    };
    let flags = args[2..].iter().fold(0, |f, a| {
      f | match a.as_str() {
        "style" => STYLE,
        "interactive" => INTERACTIVE,
        "parent" => PARENT,
        _ => 0,
      }
    });
    out.push((kind, flags));
  }
  out
}

#[test]
fn caps_list_what_the_nodes_declare() {
  let mut seen = 0;
  for (kind, flags) in SOURCES.iter().flat_map(|s| declared(s)) {
    seen += 1;
    // the widgets of `corona/base` are named without their module where only it has them
    let (plain, base) = (caps(&kind), caps(&format!("{kind}@base")));
    let listed = (plain.or(base))
      .map(|c| c | base.unwrap_or(0))
      .unwrap_or_else(|| panic!("node! makes {kind}, missing from caps.rs"));
    assert_eq!(
      listed & flags,
      flags,
      "{kind} takes more than element/caps.rs lists"
    );
  }
  assert!(seen > 80, "found only {seen} node! calls");
}

/// What a typed argument is in the probe's JS
const PROBE: &str = r#"
import { View, div } from "corona/kit";
import * as C from "corona/component";
import * as B from "corona/base";
import * as F from "corona/fps";

const MODULES = { "corona/component": C, "corona/base": B, "corona/fps": F };
const CTORS = __CTORS__;
// they read files, which a plugin directory grants
const SKIP = ["AvatarImage", "Image"];
const MEMBERS = __MEMBERS__;

export default class Main extends View {
  init(_props, cx) {
    this.fh = cx.focus_handle();
  }

  render(_cx) {
    const fh = this.fh;
    const makers = {};
    const state = (name) => {
      const patch = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n";
      for (const args of [[], ["x"], [["a"]], [1], [4], [patch], ["x", "x"]]) {
        for (const M of [C, B]) {
          try { return M[name](...args); } catch {}
          try { return new M[name](...args); } catch {}
        }
      }
      throw new Error("no state " + name);
    };
    const split = (text) => {
      const out = [];
      let depth = 0, cur = "";
      for (const c of text) {
        if ("([{<".includes(c) && !(c === "<" && cur.endsWith("=")) ) depth++;
        else if (")]}>".includes(c) && !(c === ">" && cur.endsWith("="))) depth--;
        if (c === "," && depth === 0) { out.push(cur); cur = ""; } else cur += c;
      }
      if (cur.trim()) out.push(cur);
      return out;
    };
    const val = (type) => {
      type = type.trim().replace(/^import\("[^"]*"\)\./, "");
      let m;
      if (type.startsWith("(")) return /=> (readonly )?[\w<>]*\[\]/.test(type) ? () => [] : () => div();
      if (type === "ChartRows") return [];
      if ((m = /^Element<"([^"]+)"[^>]*>(\[\])?$/.exec(type))) { const e = make(m[1]); return m[2] ? [e] : e; }
      if (type.startsWith("Element[]")) return [div()];
      if (type.startsWith("Element")) return div();
      if (type.startsWith('"')) return /"([^"]*)"/.exec(type)[1];
      if (type.startsWith("number")) return 1;
      if (type.startsWith("boolean")) return true;
      if (type.startsWith("string[]") || type.startsWith("readonly string[]")) return ["a"];
      if (type.startsWith("readonly") || type.endsWith("[]")) return [];
      if (type.startsWith("string")) return "x";
      if (/State$/.test(type)) return state(type);
      if (type === "FocusHandle") return fh;
      if (/Handle$/.test(type)) return state(type);
      if (type.startsWith("{")) return {};
      if (type === "Color") return "primary";
      if (/Length$/.test(type)) return 1;
      return "x";
    };
    const args = (text) => split(text).map((a) => val(a.slice(a.indexOf(":") + 1)));
    const build = (c) => {
      const f = MODULES[c.module][c.name];
      const a = args(c.args);
      return c.fn ? f(...a) : f.new(...a);
    };
    const make = (kind) => {
      const c = CTORS.find((c) => c.kind === kind);
      if (!c) throw new Error("no constructor of " + kind);
      return build(c);
    };
    const ok = (f) => { try { f(); return true; } catch { return false; } };
    const kindOf = (el) => {
      try { el.banner(); return "Alert"; } catch (e) {
        return /not available on (.+)$/.exec(String(e.message))?.[1] ?? String(e.message);
      }
    };

    const built = [];
    for (const c of CTORS) {
      if (SKIP.includes(c.name)) continue;
      try {
        const el = build(c);
        built.push({ name: c.name, module: c.module, declared: c.kind, kind: kindOf(el),
          style: ok(() => el.w(1)), interactive: ok(() => el.tab_index(0)), parent: ok(() => el.child("t")) });
      } catch (e) {
        built.push({ name: c.name, declared: c.kind, error: String(e.message) });
      }
    }

    // a method works on each kind it lists, and on no other that has a constructor
    const misses = [];
    const supported = [];
    const optional = (text) => split(text).some((a) => /^\s*\w+\?:/.test(a));
    const required = (text) => split(text).filter((a) => !/^\s*\w+\?:/.test(a)).join(",");
    const names = {};
    for (const m of MEMBERS) (names[m.name] ??= new Set()), m.kinds.forEach((k) => names[m.name].add(k));
    for (const m of MEMBERS) {
      const works = [];
      for (const c of CTORS) {
        if (SKIP.includes(c.name)) continue;
        let el;
        try { el = build(c); } catch { continue; }
        let threw = "";
        let ran = false;
        const variants = optional(m.args) ? [m.args, required(m.args)] : [m.args];
        for (const text of variants) {
          let a;
          try { a = args(text); } catch { continue; }
          try { el[m.name](...a); ran = true; threw = ""; break; } catch (e) { threw = String(e.message); }
        }
        const miss = /not available on/.test(threw);
        if (ran || (threw && !miss)) works.push(c.kind);
        const listed = m.kinds.includes(c.kind);
        if (listed && !ran && miss) misses.push(`${m.name} is typed on ${c.kind}, ${c.module}.${c.name} refuses it`);
        if (!listed && ran && !names[m.name].has(c.kind)) misses.push(`${m.name} runs on ${c.kind}, ${c.module}.${c.name} accepts it`);
      }
      supported.push({ name: m.name, kinds: m.kinds, works: [...new Set(works)] });
    }
    globalThis.probe = JSON.stringify({ built, misses, supported });
    return div();
  }
}
"#;

/// The constructors of the d.ts, `(module, name, kind, args, is_function)`
fn constructors() -> Vec<serde_json::Value> {
  let mut out = vec![];
  let mut module = "";
  for line in D_TS.lines() {
    if let Some(rest) = line.strip_prefix("declare module \"") {
      module = &rest[..rest.find('"').unwrap()];
      continue;
    }
    if !matches!(module, "corona/component" | "corona/base" | "corona/fps") {
      continue;
    }
    let ctor = |name: &str, kind: &str, args: &str, function: bool| serde_json::json!({ "module": module, "name": name, "kind": kind, "args": args, "fn": function });
    if let Some(rest) = line.strip_prefix("  export const ") {
      let Some((name, rest)) = rest.split_once(": ") else {
        continue;
      };
      if let Some(args) = rest.strip_prefix("{ new (") {
        if let Some((args, tail)) = args.split_once("): Element<\"") {
          out.push(ctor(name, &tail[..tail.find('"').unwrap()], args, false));
        }
      } else if let Some(rest) = rest.strip_prefix("Ctor<\"") {
        let kind = &rest[..rest.find('"').unwrap()];
        let args = rest.split_once(", [").map_or("", |(_, a)| {
          a.trim_end_matches(">;").strip_suffix(']').unwrap_or(a)
        });
        out.push(ctor(name, kind, args, false));
      }
    } else if let Some(rest) = line.strip_prefix("  export function ") {
      let parsed = rest
        .split_once('(')
        .and_then(|(name, rest)| Some((name, rest.split_once("): Element<\"")?)));
      if let Some((name, (args, tail))) = parsed.filter(|(_, (args, _))| !args.contains("DockArea"))
      {
        out.push(ctor(name, &tail[..tail.find('"').unwrap()], args, true));
      }
    }
  }
  // declared over several lines
  let extra = |module: &str, name: &str, kind: &str, args: &str, function: bool| serde_json::json!({ "module": module, "name": name, "kind": kind, "args": args, "fn": function });
  out.push(extra("corona/base", "v_virtual_list", "VirtualList@base",
    "id: string, n: number, sizes: number, key: (i: number) => string, render: (r: any) => Element[]", true));
  out.push(extra(
    "corona/component",
    "MessageScroller",
    "MessageScroller",
    "id: string, state: MessageScrollerState, render: (i: number) => Element",
    false,
  ));
  out
}

/// The kind-typed members of `Element`: `{ name, kinds, args }`
fn typed_members() -> Vec<serde_json::Value> {
  members()
    .into_iter()
    .filter_map(|(name, sig)| {
      let kinds = this_kinds(&sig)?;
      let args = &sig[sig.find("(this: Element<")? + 1..sig.rfind("): ")?];
      let args = args.split_once(">, ").map_or("", |(_, a)| a);
      Some(serde_json::json!({ "name": name, "kinds": kinds, "args": args }))
    })
    .collect()
}

/// Kinds taking `child`, though the text `child("t")` the probe tries is not what they take
const TYPED_PARENTS: &[&str] = &[
  "Resizable",
  "TabBar",
  "Accordion",
  "Stepper",
  "Sidebar",
  "SidebarMenu",
  "SidebarMenuItem",
  "Command",
  "CommandGroup",
  "NativeMenuTrigger",
  "Menu",
  "MenuBar",
  "RadioGroup",
  "Form",
  "Table",
  "TableHeader",
  "TableBody",
  "TableFooter",
  "TableRow",
  "Tree",
  "TreeItem",
  "DescriptionList",
  "Empty",
  "Settings",
  "SettingPage",
  "SettingGroup",
  "QuestionnaireItem",
  "Questionnaire",
  "Attachment",
  "Message",
  "Select",
  "Combobox",
  "Popover",
  "HoverCard",
  "Popup",
  "Dialog",
  "AlertDialog",
  "Sheet",
  "Notification",
  "Alert",
  "Tabs",
  "Scroll",
];

#[gpui::test]
fn the_runtime_matches_the_typings(cx: &mut TestAppContext) {
  let source = PROBE
    .replace(
      "__CTORS__",
      &serde_json::to_string(&constructors()).unwrap(),
    )
    .replace(
      "__MEMBERS__",
      &serde_json::to_string(&typed_members()).unwrap(),
    );
  let (view, cx) = rooted(cx, Box::leak(source.into_boxed_str()));
  let json = eval(&view, cx, "probe");
  let probe: serde_json::Value =
    serde_json::from_str(&json).unwrap_or_else(|_| panic!("the probe failed: {json}"));

  let mut failures = vec![];
  let mut by_kind: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new();
  for ctor in probe["built"].as_array().unwrap() {
    let (name, declared) = (
      ctor["name"].as_str().unwrap(),
      ctor["declared"].as_str().unwrap(),
    );
    if let Some(error) = ctor["error"].as_str() {
      failures.push(format!("{name} could not be built to be checked: {error}"));
      continue;
    }
    let kind = ctor["kind"].as_str().unwrap();
    if kind != declared.trim_end_matches("@base") {
      failures.push(format!("{name} is typed as {declared} but makes {kind}"));
    }
    let at = |k: &str| ctor[k].as_bool().unwrap();
    let e = by_kind.entry(declared.to_string()).or_default();
    *e = (
      e.0 | at("style"),
      e.1 | at("interactive"),
      e.2 | at("parent"),
    );
  }
  for (kind, (style, interactive, parent)) in by_kind {
    let listed = caps(&kind).unwrap_or(0);
    for (what, flag, found) in [
      ("style", STYLE, style),
      ("interaction", INTERACTIVE, interactive),
    ] {
      if (listed & flag != 0) != found {
        failures.push(format!(
          "{kind}: caps.rs says {} {what}, the runtime {}",
          listed & flag != 0,
          found
        ));
      }
    }
    if parent && listed & PARENT == 0 {
      failures.push(format!("{kind} takes `child`, caps.rs does not list it"));
    }
    if !parent && listed & PARENT != 0 && !TYPED_PARENTS.contains(&kind.as_str()) {
      failures.push(format!(
        "{kind} is listed as a parent, but takes no `child`"
      ));
    }
  }
  if let Some(path) = std::env::var_os("CORONA_TYPINGS_DUMP") {
    let caps: BTreeMap<_, _> = probe["built"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|c| c["error"].is_null())
      .map(|c| (c["declared"].as_str().unwrap().to_string(), c.clone()))
      .collect();
    let dump = serde_json::json!({ "built": probe["built"], "supported": probe["supported"], "unique": caps.len() });
    std::fs::write(path, dump.to_string()).unwrap();
  }
  failures.extend(
    probe["misses"]
      .as_array()
      .unwrap()
      .iter()
      .map(|m| m.as_str().unwrap().to_string()),
  );
  let shown: Vec<_> = failures.iter().take(80).cloned().collect();
  assert!(
    failures.is_empty(),
    "{} mismatches:\n{}",
    failures.len(),
    shown.join("\n")
  );
}

/// `tsc`, from `CORONA_TSC` or the path
fn tsc() -> Option<std::path::PathBuf> {
  if let Some(tsc) = std::env::var_os("CORONA_TSC") {
    return Some(tsc.into());
  }
  std::env::split_paths(&std::env::var_os("PATH")?)
    .map(|dir| dir.join("tsc"))
    .find(|tsc| tsc.is_file())
}

#[test]
fn typescript_accepts_and_rejects() {
  let Some(tsc) = tsc() else {
    eprintln!("skipped: no tsc; set CORONA_TSC to run it");
    return;
  };
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests");
  let out = std::process::Command::new(tsc)
    .args([
      "--noEmit", "--strict", "--lib", "es2022", "--target", "es2022",
    ])
    .args(["--moduleResolution", "bundler", "--module", "es2022"])
    .arg(dir.join("kit.d.ts"))
    .arg(dir.join("types/elements.ts"))
    .output()
    .expect("run tsc");
  let text = String::from_utf8_lossy(&out.stdout);
  assert!(
    out.status.success(),
    "tsc rejects the typings or accepts what it should not:\n{text}"
  );
}
