//! Immutable original bytes and byte-based locations (RFCs 0002 and 0010).

use std::sync::Arc;

#[derive(Debug)]
struct Source {
    identity: String,
    bytes: Box<[u8]>,
    lines: Vec<usize>,
}

/// An opaque equality token. It keeps the bytes alive and cannot be reassigned.
#[derive(Clone, Debug)]
pub struct SnapshotId(Arc<Source>);

impl PartialEq for SnapshotId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SnapshotId {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot(SnapshotId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    SizeUnavailable,
    InvalidRange,
}

fn representable(length: usize, maximum: usize) -> Result<(), SourceError> {
    if length > maximum {
        Err(SourceError::SizeUnavailable)
    } else {
        Ok(())
    }
}

impl Snapshot {
    /// `identity` is supplied by the caller; this does not resolve files or URIs.
    pub fn capture(identity: impl Into<String>, bytes: &[u8]) -> Result<Self, SourceError> {
        representable(bytes.len(), isize::MAX as usize)?;
        let bytes = bytes.to_vec().into_boxed_slice();
        let mut lines = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    i += 1;
                    if bytes.get(i) == Some(&b'\n') {
                        i += 1;
                    }
                    lines.push(i);
                }
                b'\n' => {
                    i += 1;
                    lines.push(i);
                }
                _ => i += 1,
            }
        }
        Ok(Self(SnapshotId(Arc::new(Source {
            identity: identity.into(),
            bytes,
            lines,
        }))))
    }

    pub fn id(&self) -> SnapshotId {
        self.0.clone()
    }

    pub fn identity(&self) -> &str {
        &self.0.0.identity
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0.0.bytes
    }

    pub fn span(&self, start: usize, end: usize) -> Result<Span, SourceError> {
        if start > end || end > self.bytes().len() {
            return Err(SourceError::InvalidRange);
        }
        Ok(Span {
            snapshot: self.clone(),
            start,
            end,
        })
    }

    pub fn eof(&self) -> Span {
        self.span(self.bytes().len(), self.bytes().len()).unwrap()
    }

    /// No projection for split scalars, CRLF interiors, or invalid line prefixes.
    pub fn coordinate(&self, byte: usize) -> Option<Coordinate> {
        let bytes = self.bytes();
        if byte > bytes.len()
            || (byte > 0 && bytes.get(byte) == Some(&b'\n') && bytes[byte - 1] == b'\r')
        {
            return None;
        }
        let line = self.0.0.lines.partition_point(|&start| start <= byte) - 1;
        let start = self.0.0.lines[line];
        let prefix = std::str::from_utf8(&bytes[start..byte]).ok()?;
        let scalars = prefix.chars().count();
        Some(Coordinate {
            line,
            scalar: scalars,
            utf8: prefix.len(),
            utf16: prefix.encode_utf16().count(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    snapshot: Snapshot,
    start: usize,
    end: usize,
}

impl Span {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn bytes(&self) -> &[u8] {
        &self.snapshot.bytes()[self.start..self.end]
    }

    /// Both endpoints or neither; no clamping or partial decoded range.
    pub fn coordinates(&self) -> Option<(Coordinate, Coordinate)> {
        Some((
            self.snapshot.coordinate(self.start)?,
            self.snapshot.coordinate(self.end)?,
        ))
    }

    pub fn encoded_u32(&self, encoding: Encoding) -> Option<((u32, u32), (u32, u32))> {
        let (start, end) = self.coordinates()?;
        Some((start.encoded_u32(encoding)?, end.encoded_u32(encoding)?))
    }
}

/// Zero-based source line and columns, not terminal display widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coordinate {
    pub line: usize,
    pub scalar: usize,
    pub utf8: usize,
    pub utf16: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

impl Coordinate {
    pub fn human(self) -> Option<(usize, usize)> {
        Some((self.line.checked_add(1)?, self.scalar.checked_add(1)?))
    }

    /// Checked projection for adapters with 32-bit coordinate fields.
    pub fn encoded_u32(self, encoding: Encoding) -> Option<(u32, u32)> {
        let column = match encoding {
            Encoding::Utf8 => self.utf8,
            Encoding::Utf16 => self.utf16,
            Encoding::Utf32 => self.scalar,
        };
        Some((self.line.try_into().ok()?, column.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_size_failure_is_not_a_source_error_or_empty_snapshot() {
        assert_eq!(representable(9, 8), Err(SourceError::SizeUnavailable));
        assert_eq!(representable(8, 8), Ok(()));
        assert_eq!(
            representable(usize::MAX, isize::MAX as usize),
            Err(SourceError::SizeUnavailable)
        );
    }
}
