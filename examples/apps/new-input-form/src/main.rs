use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| new_input_form::app::App::default().run(terminal))
}
