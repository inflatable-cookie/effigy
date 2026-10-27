use crate::tests::prelude::{parse_command, Command, HelpTopic};

#[test]
fn parse_defaults_to_help_without_command() {
    let cmd = parse_command(Vec::<String>::new()).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::General));
}