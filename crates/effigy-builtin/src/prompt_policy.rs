#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptPolicy {
    pub output_json: bool,
    pub plan: bool,
    pub explicit_non_interactive: bool,
    pub stdin_is_tty: bool,
    pub stdout_is_tty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDecision {
    Prompt,
    SuppressedByPlan,
    SuppressedByJson,
    SuppressedByExplicitNonInteractive,
    SuppressedByNonTty,
}

impl PromptPolicy {
    pub fn decide(self) -> PromptDecision {
        if self.plan {
            return PromptDecision::SuppressedByPlan;
        }
        if self.explicit_non_interactive {
            return PromptDecision::SuppressedByExplicitNonInteractive;
        }
        if self.output_json {
            return PromptDecision::SuppressedByJson;
        }
        if !self.stdin_is_tty || !self.stdout_is_tty {
            return PromptDecision::SuppressedByNonTty;
        }
        PromptDecision::Prompt
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptDecision, PromptPolicy};

    fn policy() -> PromptPolicy {
        PromptPolicy {
            output_json: false,
            plan: false,
            explicit_non_interactive: false,
            stdin_is_tty: true,
            stdout_is_tty: true,
        }
    }

    #[test]
    fn prompt_policy_requires_real_tty_and_allows_prompt() {
        assert_eq!(policy().decide(), PromptDecision::Prompt);
    }

    #[test]
    fn prompt_policy_suppresses_json_plan_explicit_and_non_tty_modes() {
        assert_eq!(
            PromptPolicy {
                output_json: true,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByJson
        );
        assert_eq!(
            PromptPolicy {
                plan: true,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByPlan
        );
        assert_eq!(
            PromptPolicy {
                explicit_non_interactive: true,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByExplicitNonInteractive
        );
        assert_eq!(
            PromptPolicy {
                output_json: true,
                explicit_non_interactive: true,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByExplicitNonInteractive
        );
        assert_eq!(
            PromptPolicy {
                stdin_is_tty: false,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByNonTty
        );
        assert_eq!(
            PromptPolicy {
                stdout_is_tty: false,
                ..policy()
            }
            .decide(),
            PromptDecision::SuppressedByNonTty
        );
    }
}
