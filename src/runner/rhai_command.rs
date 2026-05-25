use crate::runner::error::RunnerError;
use effigy_cli::{RhaiArgs, RhaiSubcommand};
use effigy_rhai::surface::rendered_signature;
use effigy_rhai::{rhai_surface_functions, rhai_surface_json};

pub(in crate::runner) fn run_rhai(args: RhaiArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        RhaiSubcommand::Surface if args.output_json => {
            serde_json::to_string_pretty(&rhai_surface_json())
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        }
        RhaiSubcommand::Surface => Ok(render_rhai_surface_text()),
    }
}

fn render_rhai_surface_text() -> String {
    let functions = rhai_surface_functions();
    let mut lines = vec![
        "Rhai Surface".to_owned(),
        format!("  Functions: {}", functions.len()),
        String::new(),
    ];
    let mut current_module = "";
    for function in functions {
        if function.module != current_module {
            current_module = function.module;
            lines.push(format!("{current_module}:"));
        }
        lines.push(format!(
            "  - {} ({})",
            rendered_signature(&function),
            function.safety
        ));
    }
    lines.join("\n")
}
