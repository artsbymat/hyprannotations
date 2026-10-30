mod app;
mod canvas;
mod config;
mod ui;
mod wayland;

use clap::Parser;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Inspect compositor protocols without creating an overlay.
    #[arg(long)]
    probe: bool,
    /// Map the overlay briefly, report configured outputs, then exit.
    #[arg(long)]
    smoke_test: bool,
    /// Write the default TOML configuration to stdout.
    #[arg(long)]
    print_config: bool,
    /// Render a toolbar preview PNG without connecting to Wayland.
    #[arg(long, value_name = "PNG")]
    preview: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.print_config {
        println!("{}", toml::to_string_pretty(&config::Settings::default())?);
        return Ok(());
    }
    if let Some(path) = args.preview {
        let mut pixels = tiny_skia::Pixmap::new(ui::WIDTH as u32, ui::HEIGHT as u32)
            .ok_or_else(|| anyhow::anyhow!("Could not allocate preview"))?;
        let app = app::App::new(config::Settings::default());
        ui::draw(&mut pixels, &app, 0.0, 0.0, 1.0);
        pixels.save_png(path)?;
        return Ok(());
    }
    wayland::run(args.probe, args.smoke_test)
}
