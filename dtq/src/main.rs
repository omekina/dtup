use clap::Parser;
use dtparse::{
    BasicFileReader, ParseErrorReport, ParsingResult, ReportDisplay, SimpleFileSystemIncluder,
    parse,
};
use std::io::{BufWriter, Stdout, Write, stdout};

#[derive(Parser)]
struct Args {
    input_file: std::path::PathBuf,
}

fn write_report(report: ParseErrorReport, stdout: &mut BufWriter<Stdout>) {
    let mut reader = BasicFileReader::default();
    ReportDisplay::new(&*report)
        .write(&mut reader, stdout)
        .unwrap();
}

fn main() {
    let args = Args::parse();

    let mut stdout = BufWriter::new(stdout());
    let start_time = std::time::Instant::now();
    let (tokens, reports) = match parse(&args.input_file, &mut SimpleFileSystemIncluder::new(".")) {
        Ok(ParsingResult::AbortCompilation(v, e) | ParsingResult::AllowCompilation(v, e)) => (v, e),
        Err(e) => panic!("io error: {e:?}"),
    };
    let execution_time = start_time.elapsed();
    for report in reports {
        write_report(report, &mut stdout);
        stdout.write_all(b"\n").unwrap();
    }
    stdout.flush().unwrap();
    eprintln!("compilation took {:?}", execution_time);
    println!("{}", tokens);
}
