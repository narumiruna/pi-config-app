#![forbid(unsafe_code)]

mod settings;
mod ui;

fn main() {
    rune::run(rune::AppSpec {
        min_size: Some((500.0, 430.0)),
        ..rune::AppSpec::new("Pi Config App", (620.0, 500.0), ui::Editor::new, ui::view)
    });
}
