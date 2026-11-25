//! The text a driver prints for each kind of finding the rule reports. Every
//! kind has its own message, so no user-facing text is built from `Debug`
//! output; a new kind fails to compile until it is given one.
use crate::analysis::scalars::ScalarFindingKind;
use crate::analysis::typed::{CoreBlocker, PublicInterfaceKind, ResultDiscardKind};

/// The message of an ownership finding, by its registered code.
pub fn ownership(code: &str) -> &'static str {
    match code {
        "R0200" => "value used after it was moved",
        "R0201" => "conflicting access",
        "R0202" => "reference escapes the value it borrows",
        "R0203" => "value released while it is borrowed",
        "R0501" => "this operation uses a capability the value does not allow",
        _ => "ownership rule violated",
    }
}

pub(super) fn scalar(kind: ScalarFindingKind) -> &'static str {
    match kind {
        ScalarFindingKind::ReturnCoverage => "not every path returns a value",
        ScalarFindingKind::TypeMismatch => "the type of this value is not the type expected here",
        ScalarFindingKind::LiteralRange => "the literal is out of range for its type",
        ScalarFindingKind::NumericContext => "nothing here gives this number a type",
        ScalarFindingKind::InsufficientContext => "the type cannot be inferred here",
        ScalarFindingKind::InvalidTarget => "this cannot be assigned to",
        ScalarFindingKind::MissingValue => "the name is not defined",
        ScalarFindingKind::MissingRequirement => "an operation this code needs is not available",
        ScalarFindingKind::NoApplicableImplementation => {
            "no implementation applies to these argument types"
        }
        ScalarFindingKind::AmbiguousImplementation => {
            "more than one implementation applies and none is the most specific"
        }
        ScalarFindingKind::DuplicateImplementation => {
            "another implementation has the same parameter pattern"
        }
        ScalarFindingKind::UnauthorizedImplementation => {
            "this implementation is not allowed in this module"
        }
        ScalarFindingKind::MissingConversionPair => {
            "no conversion applies between these types"
        }
        ScalarFindingKind::ConversionErrorMismatch => {
            "the conversion's error type is not the one the caller expects"
        }
        ScalarFindingKind::AmbiguousConversionPair => {
            "more than one conversion applies between these types"
        }
        ScalarFindingKind::ProtectedNumericGap => {
            "no conversion applies between these numeric types"
        }
        ScalarFindingKind::PrivateValue => "this value is private",
        ScalarFindingKind::StartupCapture => {
            "a named function cannot use a variable that startup declares"
        }
        ScalarFindingKind::DuplicateBinding => "this name is already bound in this scope",
        ScalarFindingKind::Arity => "the call has the wrong number of arguments",
        ScalarFindingKind::ReadOnly => "this place is read-only",
        ScalarFindingKind::GuardFallthrough => "a guard here can fall through without a result",
        ScalarFindingKind::ExpectationBoolean => "an expectation must be a bool",
        ScalarFindingKind::MissingField => "a field is missing",
        ScalarFindingKind::DuplicateField => "a field is written twice",
        ScalarFindingKind::NonExhaustive => "the match does not cover every case",
        ScalarFindingKind::RefutableBinding => "this pattern can fail to match",
        ScalarFindingKind::TagChange => "a variant cannot be changed here",
        ScalarFindingKind::InvalidTupleIndex => "the tuple has no such position",
    }
}

pub(super) fn interface(kind: PublicInterfaceKind) -> &'static str {
    match kind {
        PublicInterfaceKind::Payload => "a public declaration exposes a private payload type",
        PublicInterfaceKind::Default => "a public declaration exposes a private default",
        PublicInterfaceKind::Requirement => "a public declaration exposes a private requirement",
    }
}

pub(super) fn discard(kind: ResultDiscardKind) -> &'static str {
    match kind {
        ResultDiscardKind::Expression => "a Result is discarded by an expression statement",
        ResultDiscardKind::Replacement => "a Result is discarded by an assignment",
        ResultDiscardKind::ScopeExit => "a Result is discarded when its scope ends",
    }
}

pub fn core_blocker(blocker: CoreBlocker) -> &'static str {
    match blocker {
        CoreBlocker::Roles => "the standard library's roles could not be established",
        CoreBlocker::Providers => "the standard library's providers could not be established",
        CoreBlocker::Protection => "the standard library's protections could not be established",
        CoreBlocker::Representation => {
            "the standard library's representations could not be established"
        }
    }
}
