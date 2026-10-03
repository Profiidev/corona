use std::sync::Arc;

use gpui_kit::{AnyView, App, AppContext, Context, Render};
use serde::de::DeserializeOwned;
use tracing::error;
use uuid::Uuid;

pub trait Widget: Render {
  const NAME: &'static str;

  type Options: DeserializeOwned + Default;

  fn init(cx: &mut Context<'_, Self>, display_id: Uuid, options: Self::Options) -> Self;
}

pub type WidgetInitFn = Arc<dyn Fn(&mut App, Uuid, Option<&serde_json::Value>) -> Option<AnyView>>;

#[derive(Clone)]
pub struct WidgetData {
  pub name: String,
  init: WidgetInitFn,
}

impl WidgetData {
  pub fn new<W: Widget>() -> Self {
    Self {
      name: W::NAME.to_string(),
      init: Arc::new(|cx, display_id, options| {
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

  pub fn init(
    &self,
    cx: &mut App,
    display_id: Uuid,
    options: Option<&serde_json::Value>,
  ) -> Option<AnyView> {
    (self.init)(cx, display_id, options)
  }
}
