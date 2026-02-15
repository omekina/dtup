use crate::{
    lexer::Lexer,
    pointer_stream::{PointerTracker, RawPointerTracker},
    result::{IoError, ParseErrorReport, StreamResult},
    scopes::{ParsingResult, ScopeBuilder},
    stream_utils::StackSingleItemPrependableStream,
    string::StringDecoder,
    tokenizer::{ErrorSkipper, Tokenizer},
    tree::{DeviceTree, FailingIncluder, build_tree},
};
use std::{fs::File, io::Read, path::Path};

pub fn parse(filepath: &Path) -> Result<(DeviceTree, Vec<ParseErrorReport>), IoError> {
    let file = match File::open(filepath) {
        Ok(v) => v,
        Err(e) => return Err(e.into()),
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
    let mut scope_builder = ScopeBuilder::new(
        &mut lexer,
        filepath.extension().map(|v| v == "dtsi").unwrap_or(false),
    );
    match build_tree(&mut scope_builder, &mut FailingIncluder::default()) {
        Ok(ParsingResult::AbortCompilation(v, e)) => Ok((v, e)),
        Ok(ParsingResult::AllowCompilation(v, e)) => Ok((v, e)),
        Err(e) => Err(e),
    }
}
