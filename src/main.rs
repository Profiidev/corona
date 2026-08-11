use corona::Corona;

mod ui;

fn main() {
  tracing_subscriber::fmt::init();

  let mut corona = Corona::init().expect("Failed to initialize Corona state");
  let handle = corona.handle();

  for output in corona.outputs() {
    let handle = handle.clone();
    corona.create_widget(&output, move |b: &mut ui::bar::Bar| {
      b.on_clicked(move || {
        handle.defer(|corona| {
          let output = corona.outputs()[0].clone();
          corona.create_widget(&output, |_: &mut ui::bar::Bar| {});
        });
      });
    });
  }

  corona.run().expect("Failed to run Corona event loop");
}
