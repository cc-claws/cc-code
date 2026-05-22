mod analysis;
mod commit;
mod git;
mod render;

use clap::Parser;

/// Analyze git commit activity in a time window
#[derive(Parser, Debug)]
#[command(name = "git-stats")]
struct Cli {
    /// Number of days to look back (default: 7)
    #[arg(long, default_value = "7", conflicts_with_all = ["since", "until"])]
    days: u32,

    /// Start date (YYYY-MM-DD), exclusive with --days
    #[arg(long, conflicts_with = "days")]
    since: Option<String>,

    /// End date (YYYY-MM-DD), exclusive with --days
    #[arg(long, conflicts_with = "days")]
    until: Option<String>,

    /// Git repository path (default: current directory)
    #[arg(long, default_value = ".")]
    repo: String,
}

fn main() {
    let cli = Cli::parse();
    println!("{:?}", cli);
}
