//! The flags every subcommand grammar shares.
//!
//! `--repo`, `--json` and help mean the same thing under every command, so one
//! table owns their decoding and a sub-parser picks them up instead of
//! restating the same three arms. A sub-parser still owns its own flags, its
//! own value rules (`--pretty`, `--file`, ...) and its own precedence checks.

use std::path::PathBuf;

use crate::value_parsing::parse_repo_path;
use crate::{CliParseError, HelpTopic};

/// The typed arguments every subcommand's shared flags write.
#[derive(Debug, Default)]
pub(super) struct CommonArgs {
    pub(super) repo_override: Option<PathBuf>,
    pub(super) output_json: bool,
}

/// What one argument turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommonFlag {
    /// A shared flag decoded into [`CommonArgs`].
    Consumed,
    /// Help matched; the caller returns its own topic's panel.
    Help(HelpTopic),
    /// Not a shared flag: the caller's own table owns the argument.
    Unclaimed,
}

impl CommonArgs {
    /// Decode the shared flags. The caller passes its own help topic, so the
    /// panel a `--help` names stays the command's, never a generic one.
    pub(super) fn take<I>(
        &mut self,
        arg: &str,
        args: &mut I,
        help: HelpTopic,
    ) -> Result<CommonFlag, CliParseError>
    where
        I: Iterator<Item = String>,
    {
        match arg {
            "--repo" => {
                self.repo_override = Some(parse_repo_path(args)?);
                Ok(CommonFlag::Consumed)
            }
            "--json" => {
                self.output_json = true;
                Ok(CommonFlag::Consumed)
            }
            "--help" | "-h" => Ok(CommonFlag::Help(help)),
            _ => Ok(CommonFlag::Unclaimed),
        }
    }

    /// The same decode for a grammar that takes no repo override, so `--repo`
    /// stays that command's own refusal rather than silently becoming global.
    pub(super) fn take_output_flag(
        &mut self,
        arg: &str,
        help: HelpTopic,
    ) -> Result<CommonFlag, CliParseError> {
        match arg {
            "--json" => {
                self.output_json = true;
                Ok(CommonFlag::Consumed)
            }
            "--help" | "-h" => Ok(CommonFlag::Help(help)),
            _ => Ok(CommonFlag::Unclaimed),
        }
    }
}

/// The catch-all arm of a flag loop that accepts the shared flags: one line
/// per loop, one owner of the decision ([`CommonArgs::take`]).
macro_rules! shared_flag_arm {
    ($arg:expr, $args:expr, $common:ident, $help:expr) => {
        match $common.take($arg, $args, $help)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => {}
            CommonFlag::Unclaimed => return Err(unknown_argument($arg)),
        }
    };
}

/// The catch-all arm of a flag loop whose grammar takes no repo override.
macro_rules! shared_output_flag_arm {
    ($arg:expr, $common:ident, $help:expr) => {
        match $common.take_output_flag($arg, $help)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => {}
            CommonFlag::Unclaimed => return Err(unknown_argument($arg)),
        }
    };
}

pub(super) use shared_flag_arm;
pub(super) use shared_output_flag_arm;

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> std::vec::IntoIter<String> {
        args.iter()
            .map(|arg| (*arg).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn the_shared_flags_decode_into_one_typed_argument_set() {
        let mut common = CommonArgs::default();
        let mut args = argv(&["/repo"]);
        assert_eq!(
            common
                .take("--repo", &mut args, HelpTopic::Tasks)
                .expect("repo"),
            CommonFlag::Consumed
        );
        assert_eq!(common.repo_override, Some(PathBuf::from("/repo")));
        assert!(!common.output_json);
        assert_eq!(args.next(), None, "the path value was consumed");

        assert_eq!(
            common
                .take("--json", &mut argv(&[]), HelpTopic::Tasks)
                .expect("json"),
            CommonFlag::Consumed
        );
        assert!(common.output_json);
    }

    #[test]
    fn help_names_the_callers_own_topic() {
        for spelling in ["--help", "-h"] {
            let mut common = CommonArgs::default();
            assert_eq!(
                common
                    .take(spelling, &mut argv(&[]), HelpTopic::Deploy)
                    .expect("help"),
                CommonFlag::Help(HelpTopic::Deploy)
            );
        }
    }

    #[test]
    fn a_subcommands_own_flag_stays_unclaimed() {
        let mut common = CommonArgs::default();
        let mut args = argv(&["value"]);
        assert_eq!(
            common
                .take("--env", &mut args, HelpTopic::Deploy)
                .expect("unclaimed"),
            CommonFlag::Unclaimed
        );
        assert_eq!(
            args.next().as_deref(),
            Some("value"),
            "nothing was consumed"
        );
        assert_eq!(common.repo_override, None);
        assert!(!common.output_json);
    }

    #[test]
    fn a_missing_repo_value_is_the_shared_refusal() {
        let mut common = CommonArgs::default();
        let error = common
            .take("--repo", &mut argv(&[]), HelpTopic::Tasks)
            .expect_err("missing value");
        assert!(
            matches!(error, CliParseError::MissingRepoValue),
            "{error:?}"
        );
    }
}
