//! Error taxonomy for the native document faces. Every failure is loud
//! and typed; nothing degrades silently.

use std::fmt;

/// Failure of a parse, build or serialize call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not well-formed XML (byte offset of the problem).
    Malformed { at: usize, message: String },
    /// Input is not valid UTF-8.
    Encoding,
    /// A DOCTYPE was present but the policy forbids DOCTYPE (the default).
    DoctypeForbidden,
    /// An external entity or external DTD subset was declared. These are
    /// never resolved, whatever the policy (XXE defence).
    ExternalEntity,
    /// Entity expansion exceeded `max_entity_expansion` (or nested too deeply).
    EntityLimit,
    /// A reference to an entity that is not declared.
    UndefinedEntity(String),
    /// Element nesting exceeded `max_depth`.
    DepthLimit,
    /// A prefix is used but not bound to a namespace.
    UnboundPrefix(String),
    /// Two attributes with the same expanded name on one element.
    DuplicateAttribute(String),
    /// A built name is not a valid XML name (or breaks the namespace rules).
    InvalidName(String),
    /// A built value cannot be represented in XML (e.g. an illegal char).
    InvalidContent(String),
    /// A node handle does not belong to this document or is not an element.
    InvalidNode,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed { at, message } => {
                write!(f, "malformed XML at byte {at}: {message}")
            }
            Error::Encoding => write!(f, "input is not valid UTF-8"),
            Error::DoctypeForbidden => write!(f, "DOCTYPE is forbidden by the parser policy"),
            Error::ExternalEntity => write!(f, "external entities are never resolved"),
            Error::EntityLimit => write!(f, "entity expansion limit exceeded"),
            Error::UndefinedEntity(n) => write!(f, "undefined entity &{n};"),
            Error::DepthLimit => write!(f, "element nesting depth limit exceeded"),
            Error::UnboundPrefix(p) => write!(f, "unbound namespace prefix '{p}'"),
            Error::DuplicateAttribute(n) => write!(f, "duplicate attribute '{n}'"),
            Error::InvalidName(n) => write!(f, "invalid XML name '{n}'"),
            Error::InvalidContent(m) => write!(f, "invalid content: {m}"),
            Error::InvalidNode => write!(f, "invalid node handle"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
