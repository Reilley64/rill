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

    pub fn format(&self) -> String {
        let mut output = String::new();
        self.append_to(&mut output);
        output
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

    /// Typed display and disposal travel with the payload through erasure.
    pub fn wrapping<C: Send + Sync + 'static>(
        value: E,
        origin: SourceLocation,
        inner: OwnedErr<C>,
        display: fn(&C) -> String,
        dispose: fn(C),
    ) -> Self {
        let causes = CauseChain(Some(Box::new(Cause {
            payload: Box::new(Retained {
                value: inner.value,
                display,
                dispose,
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

    pub fn origin(&self) -> &SourceLocation {
        &self.origin
    }

    pub fn cause(&self) -> Option<&Cause> {
        self.causes.0.as_deref()
    }

    pub fn format(&self, display: fn(&E) -> String) -> String {
        let mut output = display(self.value());
        self.origin().append_to(&mut output);
        if let Some(cause) = self.cause() {
            output.push_str("\ncaused by: ");
            cause.append_chain(&mut output);
        }
        output
    }
}

/// Rust backing for conditional wrapping. The caller supplies the inner formatter
/// and disposer, and constructs the outer value and origin only on the failure edge.
pub fn wrap_result<T, E, C: Send + Sync + 'static, F: FnOnce() -> (E, SourceLocation)>(
    input: Result<T, OwnedErr<C>>,
    outer: F,
    display: fn(&C) -> String,
    dispose: fn(C),
) -> Result<T, OwnedErr<E>> {
    input.map_err(|inner| {
        let (value, origin) = outer();
        OwnedErr::wrapping(value, origin, inner, display, dispose)
    })
}

struct CauseChain(Option<Box<Cause>>);

pub struct Cause {
    payload: Box<dyn RetainedPayload>,
    origin: SourceLocation,
    next: CauseChain,
}

impl Cause {
    pub fn origin(&self) -> &SourceLocation {
        &self.origin
    }

    pub fn cause(&self) -> Option<&Cause> {
        self.next.0.as_deref()
    }

    pub fn format(&self) -> String {
        let mut output = String::new();
        self.append_chain(&mut output);
        output
    }

    fn append_chain(&self, output: &mut String) {
        let mut current = self;
        loop {
            output.push_str(&current.payload.display());
            current.origin().append_to(output);
            let Some(next) = current.cause() else {
                break;
            };
            output.push_str("\ncaused by: ");
            current = next;
        }
    }
}

impl Drop for CauseChain {
    fn drop(&mut self) {
        while let Some(head) = self.0.take() {
            let Cause {
                payload,
                origin,
                mut next,
            } = *head;
            self.0 = next.0.take();
            payload.dispose();
            drop(origin);
        }
    }
}

trait RetainedPayload: Send + Sync {
    fn display(&self) -> String;
    fn dispose(self: Box<Self>);
}

struct Retained<C> {
    value: C,
    display: fn(&C) -> String,
    dispose: fn(C),
}

impl<C: Send + Sync> RetainedPayload for Retained<C> {
    fn display(&self) -> String {
        (self.display)(&self.value)
    }

    fn dispose(self: Box<Self>) {
        (self.dispose)(self.value);
    }
}
