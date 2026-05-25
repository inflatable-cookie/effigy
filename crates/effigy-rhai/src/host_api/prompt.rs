use std::io::{self, IsTerminal, Write};

use rhai::{Engine, EvalAltResult, ImmutableString};

use crate::surface::MODULE_PROMPT;

use super::rhai_runtime_error;

pub(super) fn register_prompt_module(engine: &mut Engine) {
    engine.register_static_module(MODULE_PROMPT, std::rc::Rc::new(build_prompt_module()));
}

fn build_prompt_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "confirm",
        |message: ImmutableString, default: bool| -> Result<bool, Box<EvalAltResult>> {
            prompt_confirm(message.as_str(), default)
        },
    );
    module.set_native_fn(
        "input",
        |message: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            prompt_input(message.as_str())
        },
    );
    module
}

fn prompt_confirm(message: &str, default: bool) -> Result<bool, Box<EvalAltResult>> {
    ensure_interactive()?;
    let suffix = if default { "[Y/n]" } else { "[y/N]" };
    let response = prompt_line(&format!("{message} {suffix}: "))?;
    match response.trim().to_ascii_lowercase().as_str() {
        "" => Ok(default),
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(rhai_runtime_error("expected yes or no")),
    }
}

fn prompt_input(message: &str) -> Result<String, Box<EvalAltResult>> {
    ensure_interactive()?;
    prompt_line(&format!("{message}: "))
}

fn ensure_interactive() -> Result<(), Box<EvalAltResult>> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        Ok(())
    } else {
        Err(rhai_runtime_error(
            "prompt helpers require interactive stdin and stdout",
        ))
    }
}

fn prompt_line(prompt: &str) -> Result<String, Box<EvalAltResult>> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(prompt.as_bytes())
        .and_then(|_| stdout.flush())
        .map_err(|error| rhai_runtime_error(format!("failed to render prompt: {error}")))?;

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|error| rhai_runtime_error(format!("failed to read prompt response: {error}")))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}
