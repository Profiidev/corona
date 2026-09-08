use gpui_kit::{
  AssetSource, Result, SharedString, Window,
  assets::Assets as KitAssets,
  component::{IconNamed, icon_named},
  prelude::{IntoElement, RenderOnce},
};
use include_dir::{Dir, include_dir};
use std::borrow::Cow;

const ICONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/icons");

icon_named!(IconName, "assets/icons");

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
