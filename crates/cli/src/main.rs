#![forbid(unsafe_code)]

mod fields;
mod settings;
mod ui;

fn main() {
    rune::App::new(rune::AppSpec {
        min_size: Some((500.0, 430.0)),
        ..rune::AppSpec::new("Pi Config App", (620.0, 500.0), ui::Editor::new, ui::view)
    })
    .titlebar(rune::Titlebar::new(ui::TITLEBAR_HEIGHT))
    .run();
}
