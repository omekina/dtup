use std::{fs::File, io::Read, path::Path};

use crate::{
    pointer_stream::{PointerTracker, RawPointerTracker},
    result::{IoError, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::PrependableStream,
    string::StringDecoder,
    tokenizer::{SpanToken, Tokenizer},
};

pub fn parse(filepath: &Path) -> StreamResult<Vec<SpanToken>, StreamedError<ParseErrorReport>> {
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
    let mut tokens = Vec::new();
    for v in Tokenizer::new(&mut prependable_stream) {
        match v {
            StreamResult::Ok(v) => tokens.push(v),
            StreamResult::IoError(e) => panic!("io error: {:?}", e),
            StreamResult::ProcessingError(e) => panic!("processing error: {:?}", e),
        }
    }
    StreamResult::Ok(tokens)
}
