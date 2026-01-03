use clap::Parser;
use dtparse::parse;

#[derive(Parser)]
struct Args {
    input_file: std::path::PathBuf,
}

fn main() {
    let args = Args::parse();

    println!("{:?}", parse(&args.input_file));
}
