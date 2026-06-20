use clap::Parser;

fn main() -> anyhow::Result<()> {
    caudalghostd::run(caudalghostd::Opts::parse())
}
