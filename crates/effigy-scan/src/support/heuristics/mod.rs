mod code;
mod generated;
mod severity;
mod strings;

pub use code::{
    count_code_lines, is_literal_only_block, normalized_code_lines, trim_snippet,
    NormalizedCodeLine,
};
pub use generated::is_generated_artifact;
pub use severity::{
    classify_duplicate_block_severity, classify_severity, duplicate_block_severity_rank,
    severity_rank,
};
