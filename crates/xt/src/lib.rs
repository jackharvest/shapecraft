//! Parasolid XT transmit files, written from the vendor's published "Parasolid XT Format Reference" for
//! interoperability. No Parasolid code or library is involved.

pub mod blend;
pub mod model;
pub mod read;
pub mod schema;
pub mod write;

pub use model::{Body, Mesh, bodies};
pub use read::{Format, Node, Val, XtFile, parse};
pub use write::{write_body, write_file};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected end of transmit data")]
    Eof,
    #[error("bad transmit data: {0}")]
    Bad(String),
}

pub type Result<T> = std::result::Result<T, Error>;
