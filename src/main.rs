use corona::Corona;

mod ui;

fn main() {
  tracing_subscriber::fmt::init();

  let mut corona = Corona::init().expect("Failed to initialize Corona state");

  for output in corona.outputs() {
    corona.create_widget(&output, |b: &mut ui::bar::Bar| {
      b.on_clicked(|| {
        println!("Bar clicked!");
      });
    });
  }

  corona.run().expect("Failed to run Corona event loop");
}
