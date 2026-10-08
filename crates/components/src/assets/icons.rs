use gpui_kit::{
  AssetSource, Result, SharedString, Window,
  assets::AllAssets as KitAssets,
  component::{IconNamed, icon_named},
  prelude::{IntoElement, RenderOnce},
};
use include_dir::{Dir, include_dir};
use std::borrow::Cow;

const ICONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../assets/icons");

icon_named!(IconName, "../../assets/icons");

impl RenderOnce for IconName {
  fn render(self, _: &mut Window, _: &mut gpui_kit::App) -> impl IntoElement {
    gpui_kit::component::Icon::new(self)
  }
}

pub struct Assets;

impl AssetSource for Assets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    match path.strip_prefix("icons/").and_then(|p| ICONS.get_file(p)) {
      Some(file) => Ok(Some(Cow::Borrowed(file.contents()))),
      None => KitAssets.load(path),
    }
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let mut assets = KitAssets.list(path)?;
    assets.extend(
      ICONS
        .files()
        .map(|f| format!("icons/{}", f.path().display()))
        .filter(|p| p.starts_with(path))
        .map(SharedString::from),
    );
    Ok(assets)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn loads_embedded_icons() {
    let file = ICONS.files().next().expect("an embedded icon");
    let path = format!("icons/{}", file.path().display());
    let loaded = Assets.load(&path).unwrap().unwrap();
    assert_eq!(&*loaded, file.contents());
  }

  #[test]
  fn unknown_icons_fall_back_to_the_kit() {
    let path = "icons/surely-not-an-icon.svg";
    assert_eq!(
      Assets.load(path).ok().flatten().is_some(),
      KitAssets.load(path).ok().flatten().is_some()
    );
    // not under icons/ at all goes straight to the kit
    assert!(Assets.load("nixos.svg").ok().flatten().is_none());
  }

  #[test]
  fn list_filters_by_prefix() {
    let icons = Assets.list("icons/").unwrap();
    for file in ICONS.files() {
      let path = format!("icons/{}", file.path().display());
      assert!(icons.iter().any(|p| p.as_ref() == path), "{path}");
    }
    let none = Assets.list("no-such-dir/").unwrap();
    assert!(!none.iter().any(|p| p.starts_with("icons/")));
  }
}
