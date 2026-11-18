//! Owned error storage for the first-party runtime. This Rust interface is not a Rill ABI.

pub struct Error(String);

impl Error {
    pub fn new(message: String) -> Self {
        Self(message)
    }

    pub fn display(&self) -> String {
        self.0.clone()
    }
}

/// The compiler must supply the original source identifier and byte offset.
pub struct SourceLocation {
    source: Box<str>,
    offset: u64,
}

impl SourceLocation {
    pub fn new(source: &str, offset: u64) -> Self {
        Self {
            source: source.into(),
            offset,
        }
    }

    fn append_to(&self, output: &mut String) {
        use std::fmt::Write;
        write!(output, " ({}:byte {})", self.source, self.offset).expect("String writer");
    }
}

pub struct OwnedErr<E> {
    value: E,
    origin: SourceLocation,
    causes: CauseChain,
}

impl<E> OwnedErr<E> {
    pub fn new(value: E, origin: SourceLocation) -> Self {
        Self {
            value,
            origin,
            causes: CauseChain(None),
        }
    }

    /// A typed callback travels with its payload through erasure. No type lookup is needed.
    pub fn wrapping<C: Send + Sync + 'static>(
        value: E,
        origin: SourceLocation,
        inner: OwnedErr<C>,
        display: fn(&C) -> String,
    ) -> Self {
        let causes = CauseChain(Some(Box::new(CauseNode {
            payload: Box::new(Formatted {
                value: inner.value,
                display,
            }),
            origin: inner.origin,
            next: inner.causes,
        })));
        Self {
            value,
            origin,
            causes,
        }
    }

    pub fn value(&self) -> &E {
        &self.value
    }

    pub fn into_value(self) -> E {
        self.value
    }

    pub fn map<D>(self, apply: impl FnOnce(E) -> D) -> OwnedErr<D> {
        OwnedErr {
            value: apply(self.value),
            origin: self.origin,
            causes: self.causes,
        }
    }

    pub fn format(&self, display: fn(&E) -> String) -> String {
        let mut output = display(self.value());
        self.origin.append_to(&mut output);
        let mut current = self.causes.0.as_deref();
        while let Some(node) = current {
            output.push_str("\ncaused by: ");
            output.push_str(&node.payload.display());
            node.origin.append_to(&mut output);
            current = node.next.0.as_deref();
        }
        output
    }
}

struct CauseChain(Option<Box<CauseNode>>);

struct CauseNode {
    payload: Box<dyn DisplayPayload>,
    origin: SourceLocation,
    next: CauseChain,
}

impl Drop for CauseChain {
    fn drop(&mut self) {
        while let Some(mut head) = self.0.take() {
            self.0 = head.next.0.take();
            drop(head);
        }
    }
}

trait DisplayPayload: Send + Sync {
    fn display(&self) -> String;
}

struct Formatted<C> {
    value: C,
    display: fn(&C) -> String,
}

impl<C: Send + Sync> DisplayPayload for Formatted<C> {
    fn display(&self) -> String {
        (self.display)(&self.value)
    }
}
