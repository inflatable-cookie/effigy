//! Syntax-aware, conservative inventory of the raw runtime boundaries in 052.
//! This is QA tooling, shared by the example and its regression tests. It is
//! never linked into the runtime library.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Expr, Item, UseTree};

pub type FileCounts = BTreeMap<String, usize>;
pub type Counts = BTreeMap<String, FileCounts>;
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub schema: u16,
    pub production: Counts,
    pub tests: Counts,
}
impl Inventory {
    pub fn compare(&self, baseline: &Self) -> Result<(), String> {
        if self.schema != 1 || baseline.schema != 1 {
            return Err("unsupported kernel-ratchet schema".into());
        }
        if self.production == baseline.production {
            return Ok(());
        }
        let mut differences = Vec::new();
        let mut pairs = BTreeMap::new();
        for (file, constructs) in &baseline.production {
            for (kind, count) in constructs {
                pairs.insert((file, kind), (*count, 0));
            }
        }
        for (file, constructs) in &self.production {
            for (kind, count) in constructs {
                pairs.entry((file, kind)).or_insert((0, 0)).1 = *count;
            }
        }
        for ((file, kind), (old, new)) in pairs {
            if new > old {
                differences.push(format!("{file}: {kind} rose {old} -> {new}"));
            }
            if new < old {
                differences.push(format!(
                    "{file}: {kind} fell {old} -> {new}; lower baseline in the same change"
                ));
            }
        }
        Err(differences.join("\n"))
    }
}

pub fn inventory(root: &Path) -> Result<Inventory, String> {
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files)?;
    for entry in fs::read_dir(root.join("crates")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name() != "effigy-kernel"
            && entry.file_type().map_err(|e| e.to_string())?.is_dir()
        {
            collect(&entry.path().join("src"), &mut files)?;
        }
    }
    files.sort();
    let mut sources = BTreeMap::new();
    let mut state_modules = BTreeMap::new();
    for file in &files {
        let source = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let parsed = syn::parse_file(&source).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut finder = StateFinder(false);
        for item in &parsed.items {
            finder.visit_item(item);
        }
        state_modules.insert(file.clone(), finder.0);
        sources.insert(file.clone(), source);
    }
    let mut result = Inventory {
        schema: 1,
        ..Inventory::default()
    };
    for file in files {
        let relative = file
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let source = &sources[&file];
        // Rust's ordinary external modules inherit constants and state paths
        // from their declaring module, even when the write lives in a child
        // file (for example cache.rs -> cache/store.rs).
        // Vault paths are caller supplied, so string-based state inference
        // cannot see their writers. Keep these adopted boundaries explicit.
        let mut inherited_state = vault_storage_path(&relative);
        let mut directory = file.parent();
        while let Some(dir) = directory {
            let mut candidates = vec![dir.with_extension("rs"), dir.join("mod.rs")];
            if dir.file_name().is_some_and(|name| name == "src") {
                candidates.extend([dir.join("lib.rs"), dir.join("main.rs")]);
            }
            inherited_state |= candidates
                .iter()
                .any(|candidate| state_modules.get(candidate) == Some(&true));
            if dir.file_name().is_some_and(|name| name == "src") {
                break;
            }
            directory = dir.parent();
        }
        let mut found = scan_in_state(
            source,
            relative.split('/').any(|part| part == "tests") || relative.ends_with("/tests.rs"),
            inherited_state,
        )?;
        if vault_storage_path(&relative) && !relative.ends_with("/tests.rs") {
            // Keep explicit zero ceilings for the adopted writers. Before the
            // cutover these boundaries contained four raw write constructs.
            found.0.entry("std::fs::write".into()).or_insert(0);
            found
                .0
                .entry("std.fs.OpenOptions.write".into())
                .or_insert(0);
        }
        if !relative.ends_with("/tests.rs") {
            enforce_external_ceilings(&relative, &found.0)?;
        }
        if !found.0.is_empty() {
            result.production.insert(relative.clone(), found.0);
        }
        if !found.1.is_empty() {
            result.tests.insert(relative, found.1);
        }
    }
    Ok(result)
}
/// Boundaries that deliberately write outside the state root, accounted for by
/// name and ceiling (effigy#244).
///
/// `[scratch] root` keeps a run's marker on the volume the operator configured:
/// it is run metadata, not Effigy state, and the state root's trust policy does
/// not describe a directory on a volume the operator configured. The scanner's
/// `.effigy` heuristic therefore reads that file as a state writer, so its exact
/// writes are listed here. A construct that is not listed for a tracked file, or
/// a count above its ceiling, fails the inventory — regenerating the baseline
/// cannot widen it.
pub fn external_ceilings(relative: &str) -> Option<&'static [(&'static str, usize)]> {
    match relative {
        "src/runner/scratch.rs" => Some(&[("std::fs::File::create", 1)]),
        _ => None,
    }
}

pub fn enforce_external_ceilings(relative: &str, counts: &FileCounts) -> Result<(), String> {
    let Some(ceilings) = external_ceilings(relative) else {
        return Ok(());
    };
    for (construct, count) in counts {
        let Some(ceiling) = ceilings
            .iter()
            .find_map(|(known, ceiling)| (known == construct).then_some(*ceiling))
        else {
            return Err(format!(
                "{relative}: {construct} is not an accounted external boundary; route it \
                 through the kernel or add it here deliberately"
            ));
        };
        if *count > ceiling {
            return Err(format!(
                "{relative}: {construct} exceeds its accounted ceiling ({count} > {ceiling})"
            ));
        }
    }
    Ok(())
}

fn vault_storage_path(relative: &str) -> bool {
    matches!(
        relative,
        "src/runner/secret_vault.rs" | "crates/effigy-rhai/src/rhai_secrets.rs"
    ) || relative.starts_with("crates/effigy-secrets/src/storage")
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "ratchet refuses source symlink: {}",
                entry.path().display()
            ));
        }
        if kind.is_dir() {
            collect(&entry.path(), files)?;
        } else if entry.path().extension().is_some_and(|ext| ext == "rs") {
            files.push(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn scan_source(source: &str, test_file: bool) -> Result<(FileCounts, FileCounts), String> {
    scan_in_state(source, test_file, false)
}
fn scan_in_state(
    source: &str,
    test_file: bool,
    state: bool,
) -> Result<(FileCounts, FileCounts), String> {
    let file = syn::parse_file(source).map_err(|e| e.to_string())?;
    let mut scanner = Scanner {
        test: test_file,
        state,
        ..Scanner::default()
    };
    scanner.scope(&file.items);
    Ok((scanner.production, scanner.tests))
}
#[derive(Default)]
struct Scanner {
    aliases: BTreeMap<String, String>,
    bindings: BTreeMap<String, String>,
    production: BTreeMap<String, usize>,
    tests: BTreeMap<String, usize>,
    test: bool,
    state: bool,
}
impl Scanner {
    fn hit(&mut self, kind: &str) {
        *if self.test {
            &mut self.tests
        } else {
            &mut self.production
        }
        .entry(kind.into())
        .or_default() += 1;
    }
    fn resolve(&self, path: &syn::Path) -> String {
        let mut name = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        // Resolve nested aliases without risking a cycle from malformed imports.
        for _ in 0..16 {
            let (head, tail) = name.split_once("::").unwrap_or((&name, ""));
            let Some(alias) = self.aliases.get(head) else {
                break;
            };
            let next = if tail.is_empty() {
                alias.clone()
            } else {
                format!("{alias}::{tail}")
            };
            if next == name {
                break;
            }
            name = next;
        }
        name
    }
    fn imports(&mut self, tree: &UseTree, prefix: &str) {
        match tree {
            UseTree::Path(path) => self.imports(&path.tree, &join(prefix, &path.ident.to_string())),
            UseTree::Name(name) => {
                let id = name.ident.to_string();
                if id == "self" {
                    if let Some(last) = prefix.rsplit("::").next() {
                        self.aliases.insert(last.into(), prefix.into());
                    }
                } else {
                    self.aliases.insert(id.clone(), join(prefix, &id));
                }
            }
            UseTree::Rename(rename) => {
                let full = if rename.ident == "self" {
                    prefix.into()
                } else {
                    join(prefix, &rename.ident.to_string())
                };
                self.aliases.insert(rename.rename.to_string(), full);
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.imports(tree, prefix);
                }
            }
            UseTree::Glob(_) => {
                // Known primitive glob imports are expanded for the inventory.
                let members: &[&str] = match prefix {
                    "std::process" | "tokio::process" => &["Command"],
                    "std::fs" => &["File", "OpenOptions", "write", "rename", "remove_file"],
                    "std::env" => &["set_var", "remove_var"],
                    "libc" => &["kill", "killpg", "flock"],
                    "nix::sys::signal" => &["kill", "killpg"],
                    _ => &[],
                };
                for member in members {
                    self.aliases.insert((*member).into(), join(prefix, member));
                }
            }
        }
    }
    fn scope(&mut self, items: &[Item]) {
        let old_aliases = self.aliases.clone();
        let old_bindings = self.bindings.clone();
        let old_state = self.state;
        for item in items {
            if let Item::Use(item) = item {
                self.imports(&item.tree, "");
            }
            if let Item::Type(item) = item {
                if let syn::Type::Path(ty) = &*item.ty {
                    self.aliases
                        .insert(item.ident.to_string(), self.resolve(&ty.path));
                }
            }
        }
        let mut finder = StateFinder(false);
        for item in items {
            if !item_is_test(item) {
                finder.visit_item(item);
            }
        }
        self.state |= finder.0;
        for item in items {
            self.visit_item(item);
        }
        self.aliases = old_aliases;
        self.bindings = old_bindings;
        self.state = old_state;
    }
    fn receiver(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Path(path) => {
                let name = self.resolve(&path.path);
                self.bindings.get(&name).cloned().or(Some(name))
            }
            Expr::Call(call) => {
                let Expr::Path(path) = &*call.func else {
                    return None;
                };
                let name = self.resolve(&path.path);
                for ty in [
                    "std::process::Command",
                    "tokio::process::Command",
                    "std::fs::File",
                    "std::fs::OpenOptions",
                ] {
                    if name.starts_with(&format!("{ty}::")) {
                        return Some(ty.into());
                    }
                }
                None
            }
            Expr::MethodCall(call) => self.receiver(&call.receiver),
            Expr::Try(expr) => self.receiver(&expr.expr),
            Expr::Await(expr) => self.receiver(&expr.base),
            Expr::Reference(expr) => self.receiver(&expr.expr),
            Expr::Paren(expr) => self.receiver(&expr.expr),
            _ => None,
        }
    }
    fn call(&mut self, path: &str) {
        match path {
            "std::process::Command::new" => self.hit("std.process.Command.new"),
            "std::process::Command::spawn"
            | "std::process::Command::output"
            | "std::process::Command::status"
            | "tokio::process::Command::spawn"
            | "tokio::process::Command::output"
            | "tokio::process::Command::status" => self.hit(&path.replace("::", ".")),
            "tokio::process::Command::new" => self.hit("tokio.process.Command.new"),
            "libc::kill"
            | "libc::killpg"
            | "libc::flock"
            | "nix::sys::signal::kill"
            | "nix::sys::signal::killpg"
            | "std::env::set_var"
            | "std::env::remove_var" => self.hit(path),
            "std::fs::write"
            | "std::fs::rename"
            | "std::fs::remove_file"
            | "std::fs::File::create"
            | "std::fs::File::create_new"
                if self.state =>
            {
                self.hit(path)
            }
            _ if path.starts_with("signal_hook::")
                && (path.contains("register") || path.contains("iterator::Signals::new")) =>
            {
                self.hit("signal_hook.registration")
            }
            _ if path.starts_with("fs2::FileExt::")
                && is_lock(path.rsplit("::").next().unwrap_or("")) =>
            {
                self.hit("fs2.FileExt.lock")
            }
            _ if path.starts_with("std::fs::File::")
                && is_lock(path.rsplit("::").next().unwrap_or("")) =>
            {
                self.hit("std.fs.File.lock")
            }
            _ => {}
        }
    }
    fn macro_tokens(&mut self, tokens: proc_macro2_alias::TokenStream) {
        // Parsing macro arguments as expressions covers assert!, format!, vec!,
        // and task-local wrappers without counting comment or identifier text.
        let expressions = syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
            tokens.clone(),
        );
        if let Ok(expressions) = expressions {
            for expr in expressions {
                self.visit_expr(&expr);
            }
            return;
        }
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            self.scope(&file.items);
            return;
        }
        for token in tokens {
            match token {
                proc_macro2_alias::TokenTree::Group(group) => self.macro_tokens(group.stream()),
                proc_macro2_alias::TokenTree::Literal(literal) => {
                    if let Ok(string) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                        self.literal(&string.value());
                    }
                }
                _ => {}
            }
        }
    }
    fn literal(&mut self, value: &str) {
        for word in value.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
            if word == "pkill" || word == "killall" {
                self.hit(&format!("literal.{word}"));
            }
        }
    }
}
// QA-only token trees let the scanner inspect macro arguments.
use proc_macro2 as proc_macro2_alias;
fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}::{name}")
    }
}
fn is_lock(name: &str) -> bool {
    matches!(
        name,
        "lock"
            | "lock_shared"
            | "lock_exclusive"
            | "try_lock"
            | "try_lock_shared"
            | "try_lock_exclusive"
            | "unlock"
    )
}
fn test_cfg(attrs: &[syn::Attribute]) -> bool {
    fn requires_test(meta: &syn::Meta) -> bool {
        match meta {
            syn::Meta::Path(path) => path.is_ident("test"),
            syn::Meta::List(list) => {
                let parsed = list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                );
                if let Ok(nested) = parsed {
                    if list.path.is_ident("all") {
                        return nested.iter().any(requires_test);
                    }
                    if list.path.is_ident("any") {
                        return !nested.is_empty() && nested.iter().all(requires_test);
                    }
                }
                false
            }
            _ => false,
        }
    }
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| requires_test(&meta))
    })
}
fn item_is_test(item: &Item) -> bool {
    test_cfg(item_attrs(item))
}
fn item_attrs(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}
struct StateFinder(bool);
impl<'ast> Visit<'ast> for StateFinder {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if !attribute.path().is_ident("doc") {
            visit::visit_attribute(self, attribute);
        }
    }
    fn visit_item(&mut self, item: &'ast Item) {
        if item_is_test(item) || matches!(item, Item::Mod(_)) {
            return;
        }
        visit::visit_item(self, item);
    }
    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.0 |= literal.value().contains(".effigy");
    }
    fn visit_ident(&mut self, id: &'ast syn::Ident) {
        let id = id.to_string().to_ascii_lowercase();
        self.0 |= id.contains("effigy") && id.contains("home");
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // State path construction often lives in format! or concat!.
        self.0 |= node.tokens.to_string().contains(".effigy");
    }
}
impl<'ast> Visit<'ast> for Scanner {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if !attribute.path().is_ident("doc") {
            visit::visit_attribute(self, attribute);
        }
    }
    fn visit_item(&mut self, item: &'ast Item) {
        let old = self.test;
        self.test |= item_is_test(item);
        if let Item::Mod(module) = item {
            if let Some((_, items)) = &module.content {
                self.scope(items);
            }
        } else {
            visit::visit_item(self, item);
        }
        self.test = old;
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let aliases = self.aliases.clone();
        let bindings = self.bindings.clone();
        for stmt in &block.stmts {
            if let syn::Stmt::Item(Item::Use(item)) = stmt {
                self.imports(&item.tree, "");
            }
        }
        visit::visit_block(self, block);
        self.aliases = aliases;
        self.bindings = bindings;
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (syn::Pat::Ident(name), Some(init)) = (&local.pat, &local.init) {
            if let Some(ty) = self.receiver(&init.expr) {
                self.bindings.insert(name.ident.to_string(), ty);
            }
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let mut resolved = self.resolve(&path.path);
        if let Some(qself) = &path.qself {
            if qself.position == 0 {
                if let syn::Type::Path(ty) = &*qself.ty {
                    resolved = format!("{}::{resolved}", self.resolve(&ty.path));
                }
            }
        }
        self.call(&resolved);
        visit::visit_expr_path(self, path);
    }
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let old = self.bindings.clone();
        for input in &function.sig.inputs {
            if let syn::FnArg::Typed(arg) = input {
                let mut ty = &*arg.ty;
                while let syn::Type::Reference(reference) = ty {
                    ty = &*reference.elem;
                }
                if let (syn::Pat::Ident(name), syn::Type::Path(ty)) = (&*arg.pat, ty) {
                    self.bindings
                        .insert(name.ident.to_string(), self.resolve(&ty.path));
                }
            }
        }
        visit::visit_item_fn(self, function);
        self.bindings = old;
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        let receiver = self.receiver(&call.receiver);
        if let Some(ty) = &receiver {
            if (ty == "std::process::Command" || ty == "tokio::process::Command")
                && matches!(method.as_str(), "spawn" | "output" | "status")
            {
                self.hit(&format!("{}.{}", ty.replace("::", "."), method));
            }
            if ty == "std::fs::File" && is_lock(&method) {
                self.hit("std.fs.File.lock");
            }
            if ty == "std::fs::OpenOptions"
                && self.state
                && matches!(
                    method.as_str(),
                    "write" | "append" | "truncate" | "create" | "create_new"
                )
                && !matches!(call.args.first(), Some(Expr::Lit(lit)) if matches!(lit.lit,syn::Lit::Bool(ref b) if !b.value))
            {
                self.hit("std.fs.OpenOptions.write");
            }
        }
        // Trait methods can be invoked on File values returned by helper
        // functions; conservatively count lock methods when fs2 is in scope.
        if is_lock(&method) && self.aliases.values().any(|value| value == "fs2::FileExt") {
            self.hit("fs2.FileExt.lock");
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.literal(&literal.value());
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.macro_tokens(mac.tokens.clone());
    }
    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        let old = self.test;
        self.test |= test_cfg(&function.attrs);
        visit::visit_impl_item_fn(self, function);
        self.test = old;
    }
}
