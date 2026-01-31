use crate::pointer_stream::{PointerStream, Pos};

pub(crate) trait StreamPrepend<T> {
    /// Pushes an element into the backlog.
    ///
    /// This element will be returned if `.next()` is called directly after.
    ///
    /// # Panics
    /// If the backlog has insufficient size.
    fn push(&mut self, value: T);
}

pub(crate) trait PrependablePointer {
    /// # Panics
    /// Will panic if backlog is not empty.
    fn last_ptr(&self) -> Pos;
}

pub(crate) struct PrependableStream<T, I, const BACKLOG_SIZE: usize> {
    source: I,
    backlog: Vec<T>,
    has_ended: bool,
}

impl<T, I, const BACKLOG_SIZE: usize> PrependableStream<T, I, BACKLOG_SIZE> {
    pub(crate) fn new(source: I) -> Self {
        Self {
            source,
            backlog: Vec::with_capacity(BACKLOG_SIZE),
            has_ended: false,
        }
    }

    fn get_from_backlog(&mut self) -> Option<T> {
        match self.backlog.len() {
            0 => None,
            _ => Some(self.backlog.remove(0)),
        }
    }
}

impl<T, I, const BACKLOG_SIZE: usize> PrependableStream<T, I, BACKLOG_SIZE>
where
    I: Iterator<Item = T>,
{
    /// Awoids re-polling the source if it has already ended
    fn get_from_source(&mut self) -> Option<T> {
        if self.has_ended {
            return None;
        }
        let next = self.source.next();
        if next.is_none() {
            self.has_ended = true;
        }
        next
    }
}

impl<T, I, const BACKLOG_SIZE: usize> StreamPrepend<T> for PrependableStream<T, I, BACKLOG_SIZE> {
    fn push(&mut self, value: T) {
        if self.backlog.len() == BACKLOG_SIZE {
            panic!("tried to push to a full backlog");
        }
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
            None => self.get_from_source(),
        }
    }
}

impl<T, I: PointerStream, const BACKLOG_SIZE: usize> PrependablePointer
    for PrependableStream<T, I, BACKLOG_SIZE>
{
    fn last_ptr(&self) -> Pos {
        if !self.backlog.is_empty() {
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
        if let Some(limit) = limit
            && res.len() > limit
        {
            return Err(());
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
    use super::{PrependableStream, StreamPrepend, consume_while, skip_while};

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

    #[test]
    #[should_panic]
    fn overfull_backlog() {
        let mut source = PrependableStream::<char, _, 1>::new("test".chars().into_iter());
        source.push('a');
        source.push('b');
    }
}
