use std::io::{BufWriter, Write, stdout};

use clap::Parser;

#[derive(Parser)]
struct Args {
    input_file: std::path::PathBuf,
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut file_streamer = dtparse::BasicFileStreamer::new(&args.input_file)?;
    let mut raw_tracker = dtparse::RawPointerTracker::new(&mut file_streamer);
    let mut string_decoder = dtparse::StringDecoder::new(&mut raw_tracker);
    let tracker = dtparse::PointerTracker::new(&mut string_decoder, args.input_file);
    let res = match tracker.collect::<Result<Result<String, _>, _>>().unwrap() {
        Ok(v) => v,
        Err(report) => {
            let mut stdout = BufWriter::new(stdout());
            let mut reader = dtparse::BasicFileReader::default();
            dtparse::ReportDisplay::new(&*report).write(&mut reader, &mut stdout)?;
            stdout.flush()?;
            std::process::exit(1);
        }
    };
    println!("ok: {:?}", res);

    Ok(())
}
