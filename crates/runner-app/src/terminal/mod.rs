//! Terminal rendering: the grid element and the procedural glyph painter.

pub(crate) mod element;
pub(crate) mod find;
pub(crate) mod glyphs;
pub(crate) mod input;
#[cfg(test)]
pub(crate) mod tests;
mod viewport;

pub(crate) use element::{TerminalElement, TerminalInteraction};
pub(crate) use find::TerminalFind;
