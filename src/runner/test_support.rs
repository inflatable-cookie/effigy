pub(in crate::runner) mod builtin_contracts {
    pub(in crate::runner) use effigy_builtin::test_support::{
        parse_completion_contract_request, parse_config_contract_request,
        parse_unlock_contract_request, parse_watch_contract_request, CompletionParseContract,
        ConfigParseContract,
    };
}

pub(in crate::runner) mod catalog {
    pub(in crate::runner) use effigy_builtin::test_support::builtin_test_max_parallel;
    pub(in crate::runner) use effigy_routing::discover_catalogs;
}

pub(in crate::runner) mod execution {
    pub(in crate::runner) use super::super::execute::run_manifest_task_with_cwd;
}

pub(in crate::runner) mod parsing {
    pub(in crate::runner) use super::super::util::{
        parse_task_reference_invocation, parse_task_runtime_args,
    };
    pub(in crate::runner) use effigy_tasks::parse_task_selector;
}
