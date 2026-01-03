use crate::pointer_stream::{PointerStream, Pos};

pub(crate) trait StreamPrepend<T> {
    /// Pushes an element into the backlog.
    ///
    /// This element will be returned if `.next()` is called directly after.
    fn push(&mut self, value: T);
}

pub(crate) struct PrependableStream<T, I, const BACKLOG_SIZE: usize> {
    source: I,
    backlog: Vec<T>,
}

impl<T, I, const BACKLOG_SIZE: usize> PrependableStream<T, I, BACKLOG_SIZE> {
    pub(crate) fn new(source: I) -> Self {
        Self {
            source,
            backlog: Vec::with_capacity(BACKLOG_SIZE),
        }
    }

    fn get_from_backlog(&mut self) -> Option<T> {
        match self.backlog.len() {
            0 => None,
            _ => Some(self.backlog.remove(0)),
        }
    }
}

impl<T, I, const BACKLOG_SIZE: usize> StreamPrepend<T> for PrependableStream<T, I, BACKLOG_SIZE> {
    fn push(&mut self, value: T) {
        self.backlog.push(value);
    }
}

impl<T, I: Iterator<Item = T>, const BACKLOG_SIZE: usize> Iterator
    for PrependableStream<T, I, BACKLOG_SIZE>
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.get_from_backlog() {
            v @ Some(_) => v,
            None => self.source.next(),
        }
    }
}

impl<T, I: PointerStream, const BACKLOG_SIZE: usize> PrependableStream<T, I, BACKLOG_SIZE> {
    /// # Panics
    /// Will panic if backlog is not empty.
    pub(crate) fn last_ptr(&self) -> Pos {
        if self.backlog.len() > 0 {
            panic!("ptr getter called on a stream with non-empty backlog");
        }
        self.source.prev_ptr()
    }
}

/// # Returns
/// `Ok` if the value was successfully consumed, `Err` if the number of consumed elements was
/// greater than the limit
pub(crate) fn consume_while<T>(
    source: &mut PrependableStream<T, impl Iterator<Item = T>, 1>,
    predicate: impl Fn(&T) -> bool,
    limit: Option<usize>,
) -> Result<Vec<T>, ()> {
    let mut res = Vec::new();
    while let Some(v) = source.next() {
        match predicate(&v) {
            true => res.push(v),
            false => {
                source.push(v);
                break;
            }
        }
        if let Some(limit) = limit {
            if res.len() > limit {
                return Err(());
            }
        }
    }
    Ok(res)
}

/// # Returns
/// The number of elements skipped
pub(crate) fn skip_while<T>(
    source: &mut PrependableStream<T, impl Iterator<Item = T>, 1>,
    predicate: impl Fn(&T) -> bool,
) -> usize {
    let mut skipped = 0;
    while let Some(v) = source.next() {
        match predicate(&v) {
            true => skipped += 1,
            false => {
                source.push(v);
                break;
            }
        }
    }
    skipped
}

#[cfg(test)]
mod test {
    use crate::stream_utils::{PrependableStream, consume_while, skip_while};

    #[test]
    fn consume_ok() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(
            consume_while(&mut source, |v| *v != 's', None).unwrap(),
            ['t', 'e']
        );
        assert_eq!(source.next(), Some('s'));
    }

    #[test]
    fn consume_limit() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(consume_while(&mut source, |v| *v != 's', Some(1)), Err(()));
    }

    #[test]
    fn consume_empty_limit_ok() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(
            consume_while(&mut source, |v| *v != 't', Some(0)).unwrap(),
            []
        );
    }

    #[test]
    fn consume_empty_limit() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(consume_while(&mut source, |v| *v != 'e', Some(0)), Err(()));
    }

    #[test]
    fn consume_limit_exact() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(
            consume_while(&mut source, |v| *v != 's', Some(2)).unwrap(),
            ['t', 'e']
        );
    }

    #[test]
    fn skip() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(skip_while(&mut source, |v| *v != 's'), 2);
        assert_eq!(source.next(), Some('s'));
    }

    #[test]
    fn skip_none() {
        let mut source = PrependableStream::new("test".chars().into_iter());
        assert_eq!(skip_while(&mut source, |v| *v != 't'), 0);
        assert_eq!(source.next(), Some('t'));
    }
}
