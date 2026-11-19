use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use proc_macro2::{Span, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, Lit, Meta, Token};

pub struct ProductionSource {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Directory {
    path: PathBuf,
    relative: Option<String>,
}

impl Directory {
    fn nested(&self, name: &str) -> Self {
        let mut path = self.path.clone();
        if let Some(relative) = &self.relative {
            path.push(relative);
        }
        Self {
            path: path.join(name),
            relative: None,
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum SyntaxKind {
    Items,
    Expression,
    Statements,
}

enum Syntax {
    Items(syn::File),
    Expression(syn::Expr),
    Statements(Vec<syn::Stmt>),
}

struct ParsedSource {
    text: String,
    syntax: Syntax,
    offset: usize,
    possible: bool,
}

struct Edge {
    path: PathBuf,
    directory: Directory,
    production: bool,
    syntax: SyntaxKind,
}

#[derive(Default)]
struct Sources {
    parsed: HashMap<(PathBuf, SyntaxKind), Rc<ParsedSource>>,
    visited: HashSet<(PathBuf, Directory, bool, SyntaxKind)>,
    reached: HashSet<PathBuf>,
    production: BTreeMap<PathBuf, String>,
}

pub fn collect_production_sources(crates_root: &Path) -> Result<Vec<ProductionSource>, String> {
    let mut files = Vec::new();
    let mut roots = Vec::new();
    for entry in fs::read_dir(crates_root).map_err(|error| error.to_string())? {
        let root = entry.map_err(|error| error.to_string())?.path().join("src");
        if root.is_dir() {
            collect_files(&root, &mut HashSet::new(), &mut files)?;
            for name in ["lib.rs", "main.rs"] {
                let path = root.join(name);
                if path.is_file() {
                    roots.push(path);
                }
            }
        }
    }
    files.sort();
    files.dedup();
    let mut sources = Sources::default();
    roots.sort();
    for path in roots {
        sources.follow(
            path.clone(),
            directory_for(&path, None)?,
            true,
            SyntaxKind::Items,
        )?;
    }
    for path in files {
        if !sources.reached.contains(&path) {
            let relative = path
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| *name != "mod")
                .map(str::to_owned);
            sources.follow(
                path.clone(),
                directory_for(&path, relative)?,
                true,
                SyntaxKind::Items,
            )?;
        }
    }
    Ok(sources
        .production
        .into_iter()
        .map(|(path, source)| ProductionSource { path, source })
        .collect())
}

fn collect_files(
    root: &Path,
    visited: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !visited.insert(root.clone()) {
        return Ok(());
    }
    for entry in fs::read_dir(&root).map_err(|error| format!("{}: {error}", root.display()))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let metadata =
            fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.is_dir() {
            collect_files(&path, visited, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(
                fs::canonicalize(&path).map_err(|error| format!("{}: {error}", path.display()))?,
            );
        }
    }
    Ok(())
}

fn directory_for(path: &Path, relative: Option<String>) -> Result<Directory, String> {
    Ok(Directory {
        path: path
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", path.display()))?
            .to_owned(),
        relative,
    })
}

impl Sources {
    fn follow(
        &mut self,
        path: PathBuf,
        directory: Directory,
        production: bool,
        syntax: SyntaxKind,
    ) -> Result<(), String> {
        let mut pending = vec![Edge {
            path,
            directory,
            production,
            syntax,
        }];
        while let Some(mut edge) = pending.pop() {
            let path = fs::canonicalize(&edge.path)
                .map_err(|error| format!("{}: {error}", edge.path.display()))?;
            edge.directory.path = fs::canonicalize(&edge.directory.path)
                .map_err(|error| format!("{}: {error}", edge.directory.path.display()))?;
            if !self.visited.insert((
                path.clone(),
                edge.directory.clone(),
                edge.production,
                edge.syntax,
            )) {
                continue;
            }
            self.reached.insert(path.clone());
            let parsed = self.parse(&path, edge.syntax)?;
            let mut visitor = ScopeVisitor {
                path: &edge.path,
                directory: edge.directory,
                excluded: false,
                ranges: Vec::new(),
                edges: Vec::new(),
                error: None,
            };
            match &parsed.syntax {
                Syntax::Items(file) => visitor.visit_file(file),
                Syntax::Expression(expression) => visitor.visit_expr(expression),
                Syntax::Statements(statements) => {
                    for statement in statements {
                        visitor.visit_stmt(statement);
                    }
                }
            }
            if let Some(error) = visitor.error {
                return Err(format!("{}: {error}", path.display()));
            }
            let production = edge.production && parsed.possible;
            if production {
                let mut bytes = parsed.text.as_bytes().to_vec();
                for range in visitor.ranges {
                    let start = range.start + parsed.offset;
                    let end = range.end + parsed.offset;
                    if start >= end || end > bytes.len() {
                        return Err(format!("{}: invalid native source span", path.display()));
                    }
                    for byte in &mut bytes[start..end] {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
                if let Some(existing) = self.production.get(&path) {
                    bytes = existing
                        .bytes()
                        .zip(bytes)
                        .map(|(old, new)| if old == b' ' { new } else { old })
                        .collect();
                }
                self.production.insert(
                    path.clone(),
                    String::from_utf8(bytes).map_err(|error| error.to_string())?,
                );
            }
            pending.extend(visitor.edges.into_iter().map(|mut child| {
                child.production &= production;
                child
            }));
        }
        Ok(())
    }

    fn parse(&mut self, path: &Path, kind: SyntaxKind) -> Result<Rc<ParsedSource>, String> {
        let key = (path.to_owned(), kind);
        if let Some(parsed) = self.parsed.get(&key) {
            return Ok(Rc::clone(parsed));
        }
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut offset = 0;
        let mut possible = true;
        let syntax = match kind {
            SyntaxKind::Items => {
                let file = syn::parse_file(&text)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                offset = usize::from(text.starts_with('\u{feff}')) * 3
                    + file.shebang.as_ref().map_or(0, String::len);
                possible = configuration(&file.attrs)?.possible;
                Syntax::Items(file)
            }
            SyntaxKind::Expression => Syntax::Expression(
                syn::parse_str(&text).map_err(|error| format!("{}: {error}", path.display()))?,
            ),
            SyntaxKind::Statements => Syntax::Statements(
                syn::Block::parse_within
                    .parse_str(&text)
                    .map_err(|error| format!("{}: {error}", path.display()))?,
            ),
        };
        let parsed = Rc::new(ParsedSource {
            text,
            syntax,
            offset,
            possible,
        });
        self.parsed.insert(key, Rc::clone(&parsed));
        Ok(parsed)
    }
}

struct Configuration {
    possible: bool,
    path: Option<String>,
}

fn configuration(attributes: &[Attribute]) -> Result<Configuration, String> {
    let mut result = Configuration {
        possible: true,
        path: None,
    };
    for attribute in attributes {
        apply_attribute(&attribute.meta, true, &mut result)?;
    }
    Ok(result)
}

fn meta_list(meta: &Meta) -> Result<Vec<Meta>, String> {
    let Meta::List(list) = meta else {
        return Err("expected native cfg arguments".to_owned());
    };
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map(|values| values.into_iter().collect())
        .map_err(|error| error.to_string())
}

fn predicate(meta: &Meta) -> Result<Option<bool>, String> {
    if matches!(meta, Meta::Path(path) if path.is_ident("test")) {
        return Ok(Some(false));
    }
    if !meta.path().is_ident("all") && !meta.path().is_ident("any") && !meta.path().is_ident("not")
    {
        return Ok(None);
    }
    let values = meta_list(meta)?
        .iter()
        .map(predicate)
        .collect::<Result<Vec<_>, _>>()?;
    if meta.path().is_ident("not") {
        if values.len() != 1 {
            return Err("native cfg(not) requires one predicate".to_owned());
        }
        return Ok(values[0].map(|value| !value));
    }
    let decisive = meta.path().is_ident("any");
    if values.contains(&Some(decisive)) {
        Ok(Some(decisive))
    } else if values.contains(&None) {
        Ok(None)
    } else {
        Ok(Some(!decisive))
    }
}

fn apply_attribute(meta: &Meta, certain: bool, result: &mut Configuration) -> Result<(), String> {
    if meta.path().is_ident("cfg") {
        let values = meta_list(meta)?;
        if values.len() != 1 {
            return Err("native cfg requires one predicate".to_owned());
        }
        if certain && predicate(&values[0])? == Some(false) {
            result.possible = false;
        }
    } else if meta.path().is_ident("cfg_attr") {
        let values = meta_list(meta)?;
        let condition = predicate(values.first().ok_or("empty native cfg_attr")?)?;
        if condition != Some(false) {
            for attribute in &values[1..] {
                apply_attribute(attribute, certain && condition == Some(true), result)?;
            }
        }
    } else if meta.path().is_ident("path") {
        if !certain {
            return Err("conditional native module path is ambiguous".to_owned());
        }
        let Meta::NameValue(value) = meta else {
            return Err("native module path requires a string literal".to_owned());
        };
        let Expr::Lit(value) = &value.value else {
            return Err("native module path requires a string literal".to_owned());
        };
        let Lit::Str(value) = &value.lit else {
            return Err("native module path requires a string literal".to_owned());
        };
        if result.path.replace(value.value()).is_some() {
            return Err("multiple native module paths are ambiguous".to_owned());
        }
    }
    Ok(())
}

struct ScopeVisitor<'source> {
    path: &'source Path,
    directory: Directory,
    excluded: bool,
    ranges: Vec<Range<usize>>,
    edges: Vec<Edge>,
    error: Option<String>,
}

impl ScopeVisitor<'_> {
    fn scoped(
        &mut self,
        attributes: &[Attribute],
        span: Span,
        body: impl FnOnce(&mut Self, Option<String>),
    ) {
        if self.error.is_some() {
            return;
        }
        let config = match configuration(attributes) {
            Ok(config) => config,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let previous = self.excluded;
        self.excluded |= !config.possible;
        if self.excluded && !previous {
            self.ranges.push(span.byte_range());
        }
        body(self, config.path);
        self.excluded = previous;
    }

    fn module(&mut self, module: &syn::ItemMod, path: Option<String>) -> Result<(), String> {
        if let Some((_, items)) = &module.content {
            let directory = self.directory.clone();
            self.directory = match path {
                Some(path) => Directory {
                    path: directory.path.join(path),
                    relative: None,
                },
                None => directory.nested(&module.ident.to_string()),
            };
            for item in items {
                self.visit_item(item);
            }
            self.directory = directory;
            return Ok(());
        }
        let (path, relative) = if let Some(path) = path {
            (self.directory.path.join(path), None)
        } else {
            let name = module.ident.to_string();
            let directory = self.directory.nested("").path;
            let flat = directory.join(format!("{name}.rs"));
            let nested = directory.join(&name).join("mod.rs");
            match (flat.is_file(), nested.is_file()) {
                (true, false) => (flat, Some(name)),
                (false, true) => (nested, None),
                _ => {
                    return Err(format!(
                        "unresolved or ambiguous native module {}",
                        module.ident
                    ))
                }
            }
        };
        let directory = directory_for(&path, relative)?;
        self.edges.push(Edge {
            path,
            directory,
            production: !self.excluded,
            syntax: SyntaxKind::Items,
        });
        Ok(())
    }

    fn include(&mut self, value: &syn::Macro, syntax: SyntaxKind) {
        if self.error.is_some() {
            return;
        }
        if value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "include")
        {
            let result = syn::parse2::<syn::LitStr>(value.tokens.clone())
                .map_err(|_| "native include requires one static string literal".to_owned())
                .and_then(|literal| {
                    let path = self
                        .path
                        .parent()
                        .ok_or("source has no parent directory")?
                        .join(literal.value());
                    let directory = directory_for(&path, None)?;
                    Ok(Edge {
                        path,
                        directory,
                        production: !self.excluded,
                        syntax,
                    })
                });
            match result {
                Ok(edge) => self.edges.push(edge),
                Err(error) => self.error = Some(error),
            }
        } else if contains_module_tokens(value.tokens.clone()) {
            self.error =
                Some("opaque macro contains unresolved native module or include syntax".to_owned());
        }
    }
}

impl<'ast> Visit<'ast> for ScopeVisitor<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        let attributes: &[Attribute] = match item {
            Item::Const(value) => &value.attrs,
            Item::Enum(value) => &value.attrs,
            Item::ExternCrate(value) => &value.attrs,
            Item::Fn(value) => &value.attrs,
            Item::ForeignMod(value) => &value.attrs,
            Item::Impl(value) => &value.attrs,
            Item::Macro(value) => &value.attrs,
            Item::Mod(value) => &value.attrs,
            Item::Static(value) => &value.attrs,
            Item::Struct(value) => &value.attrs,
            Item::Trait(value) => &value.attrs,
            Item::TraitAlias(value) => &value.attrs,
            Item::Type(value) => &value.attrs,
            Item::Union(value) => &value.attrs,
            Item::Use(value) => &value.attrs,
            _ => &[],
        };
        self.scoped(attributes, item.span(), |visitor, path| {
            if let Item::Mod(module) = item {
                if let Err(error) = visitor.module(module, path) {
                    visitor.error = Some(error);
                }
            } else {
                visit::visit_item(visitor, item);
            }
        });
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attributes: &[Attribute] = match item {
            syn::ImplItem::Const(value) => &value.attrs,
            syn::ImplItem::Fn(value) => &value.attrs,
            syn::ImplItem::Type(value) => &value.attrs,
            syn::ImplItem::Macro(value) => &value.attrs,
            _ => &[],
        };
        self.scoped(attributes, item.span(), |visitor, _| {
            visit::visit_impl_item(visitor, item)
        });
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attributes: &[Attribute] = match item {
            syn::TraitItem::Const(value) => &value.attrs,
            syn::TraitItem::Fn(value) => &value.attrs,
            syn::TraitItem::Type(value) => &value.attrs,
            syn::TraitItem::Macro(value) => &value.attrs,
            _ => &[],
        };
        self.scoped(attributes, item.span(), |visitor, _| {
            visit::visit_trait_item(visitor, item)
        });
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        self.scoped(&field.attrs, field.span(), |visitor, _| {
            visit::visit_field(visitor, field)
        });
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        self.scoped(&variant.attrs, variant.span(), |visitor, _| {
            visit::visit_variant(visitor, variant)
        });
    }

    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        self.include(value, SyntaxKind::Items);
    }

    fn visit_expr_macro(&mut self, value: &'ast syn::ExprMacro) {
        self.scoped(&value.attrs, value.span(), |visitor, _| {
            visitor.include(&value.mac, SyntaxKind::Expression)
        });
    }

    fn visit_stmt_macro(&mut self, value: &'ast syn::StmtMacro) {
        self.scoped(&value.attrs, value.span(), |visitor, _| {
            visitor.include(&value.mac, SyntaxKind::Statements)
        });
    }
}

fn contains_module_tokens(tokens: proc_macro2::TokenStream) -> bool {
    let mut tokens = tokens.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident) if ident == "mod" => return true,
            TokenTree::Ident(ident) if ident == "include" => {
                if matches!(tokens.peek(), Some(TokenTree::Punct(mark)) if mark.as_char() == '!') {
                    return true;
                }
            }
            TokenTree::Group(group) if contains_module_tokens(group.stream()) => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
#[path = "source_scope_tests.rs"]
mod tests;
