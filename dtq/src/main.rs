use clap::Parser;
use dtparse::{
    BasicFileReader, ParseErrorReport, ParsingResult, ReportDisplay, SimpleFileSystemIncluder,
    parse,
};
use std::io::{BufWriter, Stdout, Write, stdout};

/// dtq (`devicetree query`)
///
/// Devicetree source validation and simple tree visualization tool.
#[derive(Parser)]
struct Args {
    /// Entrypoint devicetree file
    input_file: std::path::PathBuf,
    /// Output processing time info before tree
    #[arg(short = 't', long = "show-time")]
    show_compile_time: bool,
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
    let mut abort = false;
    let (tokens, reports) = match parse(&args.input_file, &mut SimpleFileSystemIncluder::new(".")) {
        Ok(ParsingResult::AbortCompilation(v, e)) => {
            abort = true;
            (v, e)
        },
        Ok(ParsingResult::AllowCompilation(v, e)) => (v, e),
        Err(e) => panic!("io error: {e:?}"),
    };
    let execution_time = start_time.elapsed();
    for report in reports {
        write_report(report, &mut stdout);
        stdout.write_all(b"\n").unwrap();
    }
    stdout.flush().unwrap();
    if !abort {
        if args.show_compile_time {
            eprintln!("Compilation took {:?}", execution_time);
        }
        println!("{}", tokens);
    }
}
