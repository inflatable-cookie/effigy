mod line;
mod normalize;
mod parse;
mod style;
#[cfg(test)]
mod tests;

pub use line::ansi_line;
pub(super) use normalize::normalize_terminal_payload;
