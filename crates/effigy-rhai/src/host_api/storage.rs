use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use rhai::{Array, Engine, EvalAltResult, Map};
use s3::{AddressingStyle, Auth, BlockingClient, Credentials};
use serde_json::Value;

use crate::map_to_json_object;
use crate::surface::MODULE_STORAGE;

use super::{resolve_runtime_path, rhai_runtime_error, ScriptContext};

const DEFAULT_PROVIDER: &str = "s3";
const DEFAULT_REGION: &str = "us-east-1";

pub(super) fn register_storage_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(
        MODULE_STORAGE,
        std::rc::Rc::new(build_storage_module(context)),
    );
}

fn build_storage_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();

    module.set_native_fn("provider", || -> Result<String, Box<EvalAltResult>> {
        Ok(DEFAULT_PROVIDER.to_owned())
    });
    module.set_native_fn(
        "provider",
        |options: Map| -> Result<String, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            Ok(resolve_provider(Some(&options)))
        },
    );

    let status_context = context.clone();
    module.set_native_fn("status", move || -> Result<Map, Box<EvalAltResult>> {
        storage_status(&status_context, None)
    });
    let status_context = context.clone();
    module.set_native_fn(
        "status",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_status(&status_context, Some(&options))
        },
    );

    let ls_context = context.clone();
    module.set_native_fn(
        "ls",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_ls(&ls_context, &options)
        },
    );

    let head_context = context.clone();
    module.set_native_fn(
        "head",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_head(&head_context, &options)
        },
    );

    let get_context = context.clone();
    module.set_native_fn(
        "get",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_get(&get_context, &options)
        },
    );

    let put_context = context.clone();
    module.set_native_fn(
        "put",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_put(&put_context, &options)
        },
    );

    let delete_context = context;
    module.set_native_fn(
        "delete",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            storage_delete(&delete_context, &options)
        },
    );

    module
}

fn storage_status(
    _context: &ScriptContext,
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(options)?;
    let mut map = Map::new();
    map.insert("provider".into(), config.provider.clone().into());
    map.insert("adapter".into(), "s3".into());
    map.insert("region".into(), config.region.clone().into());
    map.insert("endpoint".into(), config.endpoint.clone().into());
    map.insert(
        "bucket".into(),
        config.bucket.clone().unwrap_or_default().into(),
    );
    map.insert("path_style".into(), config.path_style.into());
    map.insert(
        "explicit_credentials".into(),
        config.explicit_credentials.into(),
    );
    map.insert(
        "ready".into(),
        (config.bucket.is_some() && !config.endpoint.is_empty()).into(),
    );
    Ok(map)
}

fn storage_ls(
    _context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(Some(options))?;
    let bucket = config.require_bucket()?;
    let prefix = string_option(options, "prefix")?;
    let delimiter = string_option(options, "delimiter")?;
    let continuation_token = string_option(options, "continuation_token")?;
    let max_keys = number_option(options, "max_keys")?;
    let client = build_client(&config)?;

    let mut request = client.objects().list_v2(&bucket);
    if let Some(prefix) = prefix {
        request = request
            .prefix(prefix)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
    }
    if let Some(delimiter) = delimiter {
        request = request
            .delimiter(delimiter)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
    }
    if let Some(token) = continuation_token {
        request = request
            .continuation_token(token)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
    }
    if let Some(max_keys) = max_keys {
        request = request
            .max_keys(max_keys)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
    }
    let output = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let objects = output
        .contents
        .iter()
        .map(|object| {
            let mut entry = Map::new();
            entry.insert("key".into(), object.key.clone().into());
            entry.insert(
                "size".into(),
                i64::try_from(object.size).unwrap_or(0).into(),
            );
            entry.insert(
                "e_tag".into(),
                object.etag.clone().unwrap_or_default().into(),
            );
            entry.into()
        })
        .collect::<Array>();

    let common_prefixes = output
        .common_prefixes
        .iter()
        .map(|prefix| prefix.clone().into())
        .collect::<Array>();

    let mut map = Map::new();
    map.insert("provider".into(), config.provider.into());
    map.insert("bucket".into(), bucket.into());
    map.insert("objects".into(), objects.into());
    map.insert("common_prefixes".into(), common_prefixes.into());
    map.insert("is_truncated".into(), output.is_truncated.into());
    map.insert(
        "next_continuation_token".into(),
        output.next_continuation_token.unwrap_or_default().into(),
    );
    Ok(map)
}

fn storage_head(
    _context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(Some(options))?;
    let bucket = config.require_bucket()?;
    let key = required_string_option(options, "key")?;
    raw_head_object_map(&config, &bucket, &key)
}

fn storage_get(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(Some(options))?;
    let bucket = config.require_bucket()?;
    let key = required_string_option(options, "key")?;
    let path = string_option(options, "path")?;
    let encoding = string_option(options, "encoding")?;
    let client = build_client(&config)?;
    let output = client
        .objects()
        .get(&bucket, &key)
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let etag = output.etag.clone();
    let content_type = output.content_type.clone();
    let content_length = output.content_length;
    let bytes = output
        .bytes()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let mut map = head_output_map(HeadObjectOutput {
        provider: config.provider.as_str(),
        bucket: &bucket,
        key: &key,
        e_tag: etag.as_deref(),
        content_type: content_type.as_deref(),
        content_length,
        version_id: None,
        metadata: &HashMap::new(),
    });
    map.insert(
        "size".into(),
        i64::try_from(bytes.len())
            .map_err(|_| rhai_runtime_error("object size exceeded Rhai integer range"))?
            .into(),
    );

    if let Some(path) = path {
        let resolved = resolve_runtime_path(&context.cwd, &path);
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                rhai_runtime_error(effigy_core::path_error_text::failed_to_write_path(
                    parent, error,
                ))
            })?;
        }
        std::fs::write(&resolved, &bytes).map_err(|error| {
            rhai_runtime_error(effigy_core::path_error_text::failed_to_write_path(
                &resolved, error,
            ))
        })?;
        map.insert("path".into(), resolved.display().to_string().into());
    } else if encoding.as_deref() == Some("base64") {
        map.insert(
            "base64_body".into(),
            base64::engine::general_purpose::STANDARD
                .encode(&bytes)
                .into(),
        );
    } else {
        let body = String::from_utf8(bytes.to_vec()).map_err(|_| {
            rhai_runtime_error("object body was not valid UTF-8; use encoding=\"base64\"")
        })?;
        map.insert("body".into(), body.into());
    }

    Ok(map)
}

fn storage_put(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(Some(options))?;
    let bucket = config.require_bucket()?;
    let key = required_string_option(options, "key")?;
    let content_type = string_option(options, "content_type")?;
    let metadata = string_map_option(options, "metadata")?;
    let body = resolve_put_body(context, options)?;
    let body_size = body.len();
    let client = build_client(&config)?;

    let mut request = client.objects().put(&bucket, &key).body_bytes(body);
    if let Some(content_type) = content_type {
        request = request
            .content_type(content_type)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
    }
    if let Some(metadata) = metadata {
        for (name, value) in metadata {
            request = request
                .metadata(name, value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
        }
    }
    let output = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let mut map = Map::new();
    map.insert("provider".into(), config.provider.into());
    map.insert("bucket".into(), bucket.into());
    map.insert("key".into(), key.into());
    map.insert("e_tag".into(), output.etag.unwrap_or_default().into());
    map.insert("version_id".into(), String::new().into());
    map.insert(
        "size".into(),
        i64::try_from(body_size)
            .map_err(|_| rhai_runtime_error("object size exceeded Rhai integer range"))?
            .into(),
    );
    map.insert("success".into(), true.into());
    Ok(map)
}

fn storage_delete(
    _context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    let config = StorageConfig::from_options(Some(options))?;
    let bucket = config.require_bucket()?;
    let key = required_string_option(options, "key")?;
    let client = build_client(&config)?;
    client
        .objects()
        .delete(&bucket, &key)
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let mut map = Map::new();
    map.insert("provider".into(), config.provider.into());
    map.insert("bucket".into(), bucket.into());
    map.insert("key".into(), key.into());
    map.insert("delete_marker".into(), false.into());
    map.insert("version_id".into(), String::new().into());
    map.insert("success".into(), true.into());
    Ok(map)
}

#[derive(Clone)]
struct StorageConfig {
    provider: String,
    region: String,
    endpoint: String,
    bucket: Option<String>,
    path_style: bool,
    explicit_credentials: bool,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    session_token: Option<String>,
    anonymous: bool,
}

impl StorageConfig {
    fn from_options(
        options: Option<&serde_json::Map<String, Value>>,
    ) -> Result<Self, Box<EvalAltResult>> {
        let provider = resolve_provider(options);
        if provider != DEFAULT_PROVIDER {
            return Err(rhai_runtime_error(format!(
                "unsupported storage provider `{provider}`"
            )));
        }

        let access_key_id = options
            .map(|options| string_option(options, "access_key_id"))
            .transpose()?
            .flatten();
        let secret_access_key = options
            .map(|options| string_option(options, "secret_access_key"))
            .transpose()?
            .flatten();
        let session_token = options
            .map(|options| string_option(options, "session_token"))
            .transpose()?
            .flatten();
        let explicit_credentials =
            access_key_id.is_some() || secret_access_key.is_some() || session_token.is_some();

        match (access_key_id.is_some(), secret_access_key.is_some()) {
            (true, true) | (false, false) => {}
            _ => {
                return Err(rhai_runtime_error(
                    "`access_key_id` and `secret_access_key` must be provided together",
                ));
            }
        }

        let region = options
            .map(|options| string_option(options, "region"))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| DEFAULT_REGION.to_owned());

        Ok(Self {
            provider,
            endpoint: options
                .map(|options| string_option(options, "endpoint"))
                .transpose()?
                .flatten()
                .unwrap_or_else(|| format!("https://s3.{region}.amazonaws.com")),
            region,
            bucket: options
                .map(|options| string_option(options, "bucket"))
                .transpose()?
                .flatten(),
            path_style: options
                .map(|options| bool_option(options, "path_style"))
                .transpose()?
                .flatten()
                .unwrap_or(false),
            explicit_credentials,
            access_key_id,
            secret_access_key,
            session_token,
            anonymous: options
                .map(|options| bool_option(options, "anonymous"))
                .transpose()?
                .flatten()
                .unwrap_or(false),
        })
    }

    fn require_bucket(&self) -> Result<String, Box<EvalAltResult>> {
        self.bucket
            .clone()
            .ok_or_else(|| rhai_runtime_error("`bucket` is required"))
    }
}

fn build_client(config: &StorageConfig) -> Result<BlockingClient, Box<EvalAltResult>> {
    let auth = if config.anonymous {
        Auth::Anonymous
    } else if let (Some(access_key_id), Some(secret_access_key)) =
        (&config.access_key_id, &config.secret_access_key)
    {
        let credentials = Credentials::new(access_key_id.clone(), secret_access_key.clone())
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
        let credentials = if let Some(token) = config.session_token.as_ref() {
            credentials
                .with_session_token(token.clone())
                .map_err(|error| rhai_runtime_error(error.to_string()))?
        } else {
            credentials
        };
        Auth::Static(credentials)
    } else {
        Auth::from_env().map_err(|error| rhai_runtime_error(error.to_string()))?
    };

    let mut builder = BlockingClient::builder(&config.endpoint)
        .map_err(|error| rhai_runtime_error(error.to_string()))?
        .region(config.region.clone())
        .auth(auth);
    if config.path_style {
        builder = builder.addressing_style(AddressingStyle::Path);
    }
    builder
        .build()
        .map_err(|error| rhai_runtime_error(error.to_string()))
}

fn raw_head_object_map(
    config: &StorageConfig,
    bucket: &str,
    key: &str,
) -> Result<Map, Box<EvalAltResult>> {
    let client = build_client(config)?;
    let presigned = client
        .objects()
        .presign_head(bucket, key)
        .expires_in(Duration::from_secs(60))
        .map_err(|error| rhai_runtime_error(error.to_string()))?
        .build()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let http = reqwest::blocking::Client::new();
    let mut request = http.head(presigned.url.as_str());
    for (name, value) in presigned.headers.iter() {
        let value = value
            .to_str()
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
        request = request.header(name.as_str(), value);
    }
    let response = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(rhai_runtime_error(format!(
            "storage head failed with HTTP status {status}"
        )));
    }

    let headers = response.headers();
    let metadata = metadata_from_headers(headers);
    Ok(head_output_map(HeadObjectOutput {
        provider: config.provider.as_str(),
        bucket,
        key,
        e_tag: header_string(headers, "etag").as_deref(),
        content_type: header_string(headers, "content-type").as_deref(),
        content_length: header_u64(headers, "content-length"),
        version_id: header_string(headers, "x-amz-version-id").as_deref(),
        metadata: &metadata,
    }))
}

struct HeadObjectOutput<'a> {
    provider: &'a str,
    bucket: &'a str,
    key: &'a str,
    e_tag: Option<&'a str>,
    content_type: Option<&'a str>,
    content_length: Option<u64>,
    version_id: Option<&'a str>,
    metadata: &'a HashMap<String, String>,
}

fn head_output_map(output: HeadObjectOutput<'_>) -> Map {
    let HeadObjectOutput {
        provider,
        bucket,
        key,
        e_tag,
        content_type,
        content_length,
        version_id,
        metadata,
    } = output;
    let mut map = Map::new();
    map.insert("provider".into(), provider.into());
    map.insert("bucket".into(), bucket.into());
    map.insert("key".into(), key.into());
    map.insert("e_tag".into(), e_tag.unwrap_or_default().to_owned().into());
    map.insert(
        "content_type".into(),
        content_type.unwrap_or_default().to_owned().into(),
    );
    map.insert(
        "content_length".into(),
        i64::try_from(content_length.unwrap_or(0))
            .unwrap_or(0)
            .into(),
    );
    map.insert(
        "version_id".into(),
        version_id.unwrap_or_default().to_owned().into(),
    );
    map.insert("metadata".into(), string_map_to_dynamic(metadata).into());
    map
}

fn metadata_from_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    const PREFIX: &str = "x-amz-meta-";

    headers
        .iter()
        .filter_map(|(name, value)| {
            name.as_str().strip_prefix(PREFIX).and_then(|metadata_key| {
                value
                    .to_str()
                    .ok()
                    .map(|metadata_value| (metadata_key.to_owned(), metadata_value.to_owned()))
            })
        })
        .collect()
}

fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn header_u64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    header_string(headers, name).and_then(|value| value.parse::<u64>().ok())
}

fn resolve_put_body(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Vec<u8>, Box<EvalAltResult>> {
    if let Some(path) = string_option(options, "path")? {
        let path = resolve_runtime_path(&context.cwd, &path);
        return std::fs::read(&path).map_err(|error| {
            rhai_runtime_error(effigy_core::path_error_text::failed_to_read_path(
                &path, error,
            ))
        });
    }
    if let Some(body) = string_option(options, "body")? {
        return Ok(body.into_bytes());
    }
    if let Some(body) = string_option(options, "base64_body")? {
        return base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|error| rhai_runtime_error(error.to_string()));
    }
    Err(rhai_runtime_error(
        "one of `path`, `body`, or `base64_body` is required",
    ))
}

fn resolve_provider(options: Option<&serde_json::Map<String, Value>>) -> String {
    options
        .and_then(|options| options.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROVIDER)
        .to_owned()
}

fn required_string_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Box<EvalAltResult>> {
    string_option(options, key)?.ok_or_else(|| rhai_runtime_error(format!("`{key}` is required")))
}

fn string_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a string"))),
    }
}

fn bool_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a bool"))),
    }
}

fn number_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<u32>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| {
                rhai_runtime_error(format!("`{key}` must be a 32-bit unsigned integer"))
            }),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be an integer"))),
    }
}

fn string_map_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<HashMap<String, String>>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Object(values)) => values
            .iter()
            .map(|(name, value)| {
                value
                    .as_str()
                    .map(|value| (name.clone(), value.to_owned()))
                    .ok_or_else(|| rhai_runtime_error(format!("`{key}` values must be strings")))
            })
            .collect::<Result<HashMap<_, _>, _>>()
            .map(Some),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a map"))),
    }
}

fn string_map_to_dynamic(values: &HashMap<String, String>) -> Map {
    let mut map = Map::new();
    for (key, value) in values {
        map.insert(key.clone().into(), value.clone().into());
    }
    map
}
