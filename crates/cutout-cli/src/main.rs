use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "cutout", about = "Cutout CLI scaffold")]
struct Cli;

fn main() {
    let _ = Cli::parse();
}
