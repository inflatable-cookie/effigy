//! Thin adapter — the parser lives in `effigy-env::dotenv`.
//!
//! Moved out in batch 241.

pub(in crate::runner) use effigy_env::dotenv::parse_dotenv_entries;
