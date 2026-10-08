pub mod animation;
pub mod assets;
pub mod async_listener;
pub mod components;

#[cfg(test)]
pub(crate) mod test_view {
  use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Context, Entity, IntoElement, Render,
    TestAppContext, Window, WindowOptions,
  };

  type Build = Box<dyn Fn(&mut Window, &mut App) -> AnyElement>;

  /// Draws whatever `build` returns, every frame
  pub struct View(Build);

  impl Render for View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
      (self.0)(window, cx)
    }
  }

  /// A kit-initialized app with the default config and a window drawing `build`
  pub fn open(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
  ) -> (AnyWindowHandle, Entity<View>) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
        cx.new(|_| View(Box::new(build)))
      })
      .unwrap()
    })
  }

  /// Completes a frame
  pub fn draw(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
      .unwrap();
  }
}
