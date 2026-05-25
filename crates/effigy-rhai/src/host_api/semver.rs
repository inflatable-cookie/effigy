use rhai::{Engine, EvalAltResult, ImmutableString, Map};
use semver::{Version, VersionReq};

use crate::surface::MODULE_SEMVER;

use super::rhai_runtime_error;

pub(super) fn register_semver_module(engine: &mut Engine) {
    engine.register_static_module(MODULE_SEMVER, std::rc::Rc::new(build_semver_module()));
}

fn build_semver_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "parse",
        |version: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            parse_version(version.as_str()).map(version_to_map)
        },
    );
    module.set_native_fn(
        "valid",
        |version: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(Version::parse(version.as_str()).is_ok())
        },
    );
    module.set_native_fn(
        "compare",
        |left: ImmutableString, right: ImmutableString| -> Result<i64, Box<EvalAltResult>> {
            let left = parse_version(left.as_str())?;
            let right = parse_version(right.as_str())?;
            Ok(match left.cmp(&right) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })
        },
    );
    module.set_native_fn(
        "satisfies",
        |version: ImmutableString,
         requirement: ImmutableString|
         -> Result<bool, Box<EvalAltResult>> {
            let version = parse_version(version.as_str())?;
            let requirement = VersionReq::parse(requirement.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(requirement.matches(&version))
        },
    );
    module.set_native_fn(
        "bump_major",
        |version: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let version = parse_version(version.as_str())?;
            Ok(format!("{}.0.0", version.major + 1))
        },
    );
    module.set_native_fn(
        "bump_minor",
        |version: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let version = parse_version(version.as_str())?;
            Ok(format!("{}.{}.0", version.major, version.minor + 1))
        },
    );
    module.set_native_fn(
        "bump_patch",
        |version: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let version = parse_version(version.as_str())?;
            Ok(format!(
                "{}.{}.{}",
                version.major,
                version.minor,
                version.patch + 1
            ))
        },
    );
    module
}

fn parse_version(raw: &str) -> Result<Version, Box<EvalAltResult>> {
    Version::parse(raw.trim_start_matches('v'))
        .map_err(|error| rhai_runtime_error(error.to_string()))
}

fn version_to_map(version: Version) -> Map {
    let mut map = Map::new();
    map.insert("major".into(), (version.major as i64).into());
    map.insert("minor".into(), (version.minor as i64).into());
    map.insert("patch".into(), (version.patch as i64).into());
    map.insert("pre".into(), version.pre.to_string().into());
    map.insert("build".into(), version.build.to_string().into());
    map.insert("normalized".into(), version.to_string().into());
    map
}
