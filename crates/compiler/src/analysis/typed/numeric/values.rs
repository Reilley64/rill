//! Linked value operations. Rust types here do not define a Rill ABI.
use rill_collection::List;

/// Independent owned byte storage for the linked operations.
pub struct Bytes(Box<[u8]>);

impl Bytes {
    pub fn take_list(input: List<u8>) -> Self {
        Self(input.into_owned_iter().collect())
    }

    pub fn copy_list(input: &List<u8>) -> Self {
        Self(input.iter().copied().collect())
    }

    pub fn into_list(self) -> List<u8> {
        List::from_elements(self.0.into_vec())
    }

    pub fn to_list(&self) -> List<u8> {
        List::from_elements(self.0.iter().copied())
    }

    pub fn view(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Utf8Error {
    pub valid_up_to: u64,
    pub error_length: Option<u64>,
}

pub fn text_bytes(text: &str) -> &[u8] {
    text.as_bytes()
}

pub fn decode_view(input: &[u8]) -> Result<&str, Utf8Error> {
    // Rust's validator reports the same maximal prefix and incomplete tail as RFC 0006.
    std::str::from_utf8(input).map_err(|error| Utf8Error {
        valid_up_to: error.valid_up_to() as u64,
        error_length: error.error_len().map(|length| length as u64),
    })
}

pub fn decode_owned(input: &[u8]) -> Result<String, Utf8Error> {
    decode_view(input).map(str::to_owned)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidScalar;

pub fn code_point(input: char) -> u32 {
    u32::from(input)
}

pub fn unicode_scalar(input: u32) -> Result<char, InvalidScalar> {
    char::from_u32(input).ok_or(InvalidScalar)
}
