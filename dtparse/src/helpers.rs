use crate::{
    base_tree::{IgnorantIncluder, RootScope, preprocess_tree},
    lexer::Lexer,
    pointer_stream::{PointerTracker, RawPointerTracker},
    result::{IoError, ParseErrorReport, StreamResult},
    stream_utils::StackSingleItemPrependableStream,
    string::StringDecoder,
    tokenizer::{ErrorSkipper, Tokenizer},
};
use std::{fs::File, io::Read, path::Path};

pub fn parse(filepath: &Path) -> StreamResult<(Option<RootScope>, Vec<ParseErrorReport>), ()> {
    let file = match File::open(filepath) {
        Ok(v) => v,
        Err(e) => return StreamResult::IoError(e.into()),
    };
    let mut file_stream = file.bytes().map(|v| v.map_err(IoError::from));
    let mut raw_pointer_tracker = RawPointerTracker::new(&mut file_stream);
    let mut string_decoder = StringDecoder::new(&mut raw_pointer_tracker);
    let mut pointer_tracker = PointerTracker::new(&mut string_decoder, filepath.to_path_buf());
    let mut prependable_stream: StackSingleItemPrependableStream<
        StreamResult<char, ParseErrorReport>,
        _,
    > = StackSingleItemPrependableStream::new(&mut pointer_tracker);
    let mut tokenizer = Tokenizer::new(&mut prependable_stream);
    let mut error_skipper = ErrorSkipper::new(&mut tokenizer);
    let mut prependable_stream = StackSingleItemPrependableStream::new(&mut error_skipper);
    let mut lexer = Lexer::new(&mut prependable_stream);
    let mut prependable_stream = StackSingleItemPrependableStream::new(&mut lexer);
    let (res, _, errors) = match preprocess_tree(
        &mut prependable_stream,
        filepath.extension().map(|v| v == "dtsi").unwrap_or(false),
        &mut IgnorantIncluder,
    ) {
        StreamResult::Ok(v) => v,
        StreamResult::IoError(e) => return StreamResult::IoError(e),
        StreamResult::ProcessingError(e) => {
            return StreamResult::Ok((None, e));
        }
    };
    StreamResult::Ok((Some(res), errors))
}
