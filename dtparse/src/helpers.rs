use std::{fs::File, io::Read, path::Path};

use crate::{
    lexer::{Lexer, LexerToken},
    pointer_stream::{PointerTracker, RawPointerTracker},
    result::{IoError, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::PrependableStream,
    string::StringDecoder,
    tokenizer::{SpanToken, Tokenizer},
};

pub fn parse(filepath: &Path) -> StreamResult<Vec<LexerToken>, Vec<ParseErrorReport>> {
    let file = match File::open(filepath) {
        Ok(v) => v,
        Err(e) => return StreamResult::IoError(e.into()),
    };
    let mut file_stream = file.bytes().map(|v| v.map_err(|e| IoError::from(e)));
    let mut raw_pointer_tracker = RawPointerTracker::new(&mut file_stream);
    let mut string_decoder = StringDecoder::new(&mut raw_pointer_tracker);
    let pointer_tracker = PointerTracker::new(&mut string_decoder, filepath.to_path_buf());
    let mut prependable_stream: PrependableStream<StreamResult<char, ParseErrorReport>, _, 1> =
        PrependableStream::new(pointer_tracker);
    let tokenizer = Tokenizer::new(&mut prependable_stream);
    let mut prependable_stream: PrependableStream<
        StreamResult<SpanToken, StreamedError<ParseErrorReport>>,
        _,
        1,
    > = PrependableStream::new(tokenizer);
    let mut lexed = Vec::new();
    let mut reports = Vec::new();
    for v in Lexer::new(&mut prependable_stream) {
        match v {
            StreamResult::Ok(v) => {
                if let Some(r) = v.reports {
                    reports.extend(r.into_iter());
                }
                lexed.push(v.token);
            }
            StreamResult::IoError(e) => return StreamResult::IoError(e),
            StreamResult::ProcessingError(StreamedError::ShouldEnd(e)) => {
                reports.push(e);
                break;
            }
            StreamResult::ProcessingError(StreamedError::CanContinue(e)) => reports.push(e),
        }
    }
    match reports.len() {
        0 => StreamResult::Ok(lexed),
        _ => StreamResult::ProcessingError(reports),
    }
}
