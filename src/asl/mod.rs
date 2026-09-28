//! ASL の定義（`definition`）と JSONPath の部分集合（`path`）。

pub mod definition;
pub mod path;

pub use definition::{Definition, parse};
