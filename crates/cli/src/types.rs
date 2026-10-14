use std::path::PathBuf;

use clap::Args;

/// Writes each plugin's `gpui-kit.d.ts`, as loading it does; no running shell needed
#[derive(Args)]
pub struct TypesCommand {
  /// The plugin directories, each with a `plugin.toml`
  #[arg(required = true)]
  dirs: Vec<PathBuf>,
}

impl TypesCommand {
  pub fn execute(self) {
    // headless, so it runs without a display, e.g. in CI
    gpui_kit::platform::headless().run(move |cx| {
      let code = match corona_script::write_plugin_types(&self.dirs, cx) {
        Ok(written) => {
          for path in written {
            println!("{}", path.display());
          }
          0
        }
        Err(e) => {
          eprintln!("error: {e:#}");
          1
        }
      };
      std::process::exit(code);
    });
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    path::{Path, PathBuf},
  };

  use gpui_kit::{self as gpui, TestAppContext};

  const VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";

export default class Main extends View {
  render(_cx) {
    return v_flex().child("plugin");
  }
}
"#;

  fn plugin(root: &Path, id: &str, extra: &str) -> PathBuf {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    let manifest =
      format!("id = \"{id}\"\nname = \"{id}\"\n\n[widgets.main]\nview = \"main.js\"\n\n{extra}");
    fs::write(dir.join("plugin.toml"), manifest).unwrap();
    fs::write(dir.join("main.js"), VIEW).unwrap();
    dir
  }

  fn settings(dir: &Path) -> String {
    let dts = fs::read_to_string(dir.join("gpui-kit.d.ts")).unwrap();
    let start = dts.find("declare module \"corona/settings\"").unwrap();
    dts[start..start + dts[start..].find("\n}").unwrap()].to_string()
  }

  #[gpui::test]
  fn writes_each_plugins_types(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let a = plugin(root.path(), "a", "");
    let b = plugin(
      root.path(),
      "b",
      "[[settings]]\nkey = \"units\"\nlabel = \"Units\"\ntype = \"text\"\ndefault = \"\"",
    );

    let dirs = [a.clone(), b.clone()];
    let written = cx
      .update(|cx| corona_script::write_plugin_types(&dirs, cx))
      .unwrap();

    assert!(written.contains(&a.join("gpui-kit.d.ts")), "{written:?}");
    assert!(written.contains(&b.join("gpui-kit.d.ts")), "{written:?}");
    assert!(!settings(&a).contains("\"units\""));
    assert!(
      settings(&b).contains("\"units\": string;"),
      "{}",
      settings(&b)
    );
  }

  #[gpui::test]
  fn fails_naming_a_dir_without_a_plugin(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("none");
    let dirs = [missing.clone()];
    let error = cx
      .update(|cx| corona_script::write_plugin_types(&dirs, cx))
      .unwrap_err();
    assert!(
      format!("{error:#}").contains(&missing.display().to_string()),
      "{error:#}"
    );
  }
}
