use std::path::PathBuf;

/// A parsed `.env.schema` file containing environment variable declarations.
#[derive(Debug)]
pub struct EnvSchema {
    pub entries: Vec<EnvSchemaEntry>,
    pub source_path: PathBuf,
}

/// A single environment variable declaration in the schema.
#[derive(Debug)]
pub struct EnvSchemaEntry {
    pub key: String,
    pub value: Option<EnvValueExpr>,
    pub annotations: EntryAnnotations,
    pub line: usize,
}

/// Annotations parsed from comment lines preceding a variable declaration.
#[derive(Debug, Default)]
pub struct EntryAnnotations {
    pub env_type: Option<EnvType>,
    pub required: Option<bool>,
    pub sensitive: bool,
    pub description: Option<String>,
}

/// Type annotations constraining the value of an environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvType {
    Port,
    Url,
    String {
        min_len: Option<usize>,
        max_len: Option<usize>,
    },
    Enum(Vec<String>),
}

/// A value expression on the right side of `KEY=...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvValueExpr {
    /// A plain literal value like `3000` or `https://example.com`.
    Literal(String),
    /// An `exec('command')` expression that runs a shell command.
    Exec(String),
    /// An `env('VAR_NAME')` reference to another environment variable.
    EnvRef(String),
    /// A template string containing `${VAR}` interpolation.
    Template(Vec<TemplatePart>),
}

/// A segment of a template value expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Literal text between variable references.
    Literal(String),
    /// A `${VAR}` reference.
    VarRef(String),
}
