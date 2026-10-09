use std::sync::Arc;

use gpui_kit::{AnyView, App, AppContext, Context, Render, Window};
use serde::de::DeserializeOwned;
use tracing::error;
use uuid::Uuid;

pub trait Widget: Render {
  const NAME: &'static str;

  type Options: DeserializeOwned + Default;

  fn init(cx: &mut Context<'_, Self>, display_id: Uuid, options: Self::Options) -> Self;
}

pub type WidgetInitFn =
  Arc<dyn Fn(&mut Window, &mut App, Uuid, Option<&serde_json::Value>) -> Option<AnyView>>;

#[derive(Clone)]
pub struct WidgetData {
  pub name: String,
  init: WidgetInitFn,
}

impl WidgetData {
  pub fn new<W: Widget>() -> Self {
    Self {
      name: W::NAME.to_string(),
      init: Arc::new(|_, cx, display_id, options| {
        let options = match options {
          None => W::Options::default(),
          Some(value) => match serde_json::from_value(value.clone()) {
            Ok(options) => options,
            Err(e) => {
              error!("invalid options for widget {}: {e}", W::NAME);
              return None;
            }
          },
        };
        Some(cx.new(|cx| W::init(cx, display_id, options)).into())
      }),
    }
  }

  /// A widget built by `init`, given the bar's window; `None` skips it
  pub fn from_fn(
    name: impl Into<String>,
    init: impl Fn(&mut Window, &mut App, Uuid, Option<&serde_json::Value>) -> Option<AnyView> + 'static,
  ) -> Self {
    Self {
      name: name.into(),
      init: Arc::new(init),
    }
  }

  pub fn init(
    &self,
    window: &mut Window,
    cx: &mut App,
    display_id: Uuid,
    options: Option<&serde_json::Value>,
  ) -> Option<AnyView> {
    (self.init)(window, cx, display_id, options)
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{Entity, TestAppContext};

  use super::*;
  use crate::test_support::{Label, LabelOptions};

  fn built(options: Option<serde_json::Value>, cx: &mut TestAppContext) -> Option<LabelOptions> {
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
      let view = WidgetData::new::<Label>().init(window, cx, Uuid::nil(), options.as_ref())?;
      let label: Entity<Label> = view.downcast().ok()?;
      Some(label.read(cx).0.clone())
    })
  }

  #[gpui_kit::test]
  fn options_parse_or_default_or_reject(cx: &mut TestAppContext) {
    assert_eq!(WidgetData::new::<Label>().name, "label");
    assert_eq!(built(None, cx), Some(LabelOptions::default()));
    assert_eq!(
      built(Some(serde_json::json!({ "text": "hi" })), cx),
      Some(LabelOptions { text: "hi".into() })
    );
    assert_eq!(
      built(Some(serde_json::json!({})), cx),
      Some(LabelOptions::default())
    );
    assert_eq!(built(Some(serde_json::json!({ "text": 3 })), cx), None);
    assert_eq!(built(Some(serde_json::json!("nope")), cx), None);
  }

  #[gpui_kit::test]
  fn from_fn_gets_the_window_and_options(cx: &mut TestAppContext) {
    let data = WidgetData::from_fn("plugin:w", |window, cx, _, options| {
      let text = format!("{options:?} {:?}", window.window_handle().window_id());
      Some(cx.new(|_| Label(LabelOptions { text })).into())
    });
    assert_eq!(data.name, "plugin:w");
    let cx = cx.add_empty_window();
    let (text, id) = cx.update(|window, cx| {
      let view = data.init(window, cx, Uuid::nil(), None).unwrap();
      let label: Entity<Label> = view.downcast().unwrap();
      (
        label.read(cx).0.text.clone(),
        window.window_handle().window_id(),
      )
    });
    assert_eq!(text, format!("None {id:?}"));
    let none = WidgetData::from_fn("x", |_, _, _, _| None);
    assert!(
      cx.update(|window, cx| none.init(window, cx, Uuid::nil(), None))
        .is_none()
    );
  }
}
