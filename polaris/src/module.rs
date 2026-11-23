use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ast::{Stmt, StmtKind};
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::Lexer;
use crate::parser::Parser;

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub id: String,
    pub display_name: String,
    pub source: String,
    pub stmts: Vec<Stmt>,
    pub imports: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ModuleProgram {
    pub modules: HashMap<String, LoadedModule>,
    pub load_order: Vec<String>,
    pub entry_id: String,
}

impl ModuleProgram {
    pub fn format_diagnostic(&self, diag: &Diagnostic) -> String {
        if let Some(file_key) = &diag.file {
            if let Some(m) = self
                .modules
                .values()
                .find(|m| &m.id == file_key || &m.display_name == file_key)
            {
                return diag.format_with_source(&m.source);
            }
        }
        if let Some(entry) = self.modules.get(&self.entry_id) {
            return diag.format_with_source(&entry.source);
        }
        diag.to_string()
    }
}

pub struct ModuleLoader {
    virtual_files: HashMap<String, String>,
    modules: HashMap<String, LoadedModule>,
    loading_stack: Vec<String>,
    load_order: Vec<String>,
}

impl ModuleLoader {
    pub fn new(virtual_files: Vec<(&str, &str)>) -> Self {
        let mut map = HashMap::new();
        for (name, src) in virtual_files {
            map.insert(normalize_path_str(name), src.to_string());
        }
        Self {
            virtual_files: map,
            modules: HashMap::new(),
            loading_stack: Vec::new(),
            load_order: Vec::new(),
        }
    }

    pub fn load_entry_source(
        mut self,
        entry_source: &str,
        entry_path: Option<&Path>,
    ) -> Result<ModuleProgram, String> {
        let (entry_id, display_name, base_dir) = match entry_path {
            Some(p) => {
                let canon = fs::canonicalize(p)
                    .map(|cp| cp.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| normalize_path(p));
                let dir = p.parent().map(|d| d.to_path_buf());
                (
                    canon,
                    p.to_string_lossy().into_owned(),
                    dir,
                )
            }
            None => (
                "__main__".to_string(),
                "<main>".to_string(),
                None,
            ),
        };

        self.load_module_with_source(
            entry_id.clone(),
            display_name,
            entry_source.to_string(),
            base_dir.as_deref(),
        )?;

        Ok(ModuleProgram {
            modules: self.modules,
            load_order: self.load_order,
            entry_id,
        })
    }

    fn load_module_with_source(
        &mut self,
        id: String,
        display_name: String,
        source: String,
        base_dir: Option<&Path>,
    ) -> Result<String, String> {
        if self.modules.contains_key(&id) {
            return Ok(id);
        }
        if self.loading_stack.contains(&id) {
            let mut chain = self.loading_stack.clone();
            chain.push(id);
            return Err(format!("Circular import detected: {}", chain.join(" -> ")));
        }

        self.loading_stack.push(id.clone());

        let tokens = Lexer::new(&source)
            .tokenize()
            .map_err(|d| d.with_file(&display_name).format_with_source(&source))?;
        let stmts = Parser::new(tokens)
            .parse_program()
            .map_err(|d| d.with_file(&display_name).format_with_source(&source))?;

        let mut imports = HashMap::new();
        let mut seen_aliases = HashSet::new();

        for stmt in &stmts {
            if let StmtKind::Import {
                path,
                is_file_path,
                alias,
            } = &stmt.node
            {
                if !seen_aliases.insert(alias.clone()) {
                    let diag = Diagnostic::new(
                        "module",
                        stmt.span,
                        format!("Duplicate import alias '{}'", alias),
                    )
                    .with_file(&display_name)
                    .with_hint("Use 'import ... as <other_name>' to give each module a unique alias");
                    return Err(diag.format_with_source(&source));
                }

                let (dep_id, dep_display, dep_source, dep_dir) =
                    self.resolve_and_read(path, *is_file_path, base_dir, stmt.span, &display_name, &source)?;

                if self.loading_stack.contains(&dep_id) {
                    let mut chain = self.loading_stack.clone();
                    chain.push(dep_id);
                    let diag = Diagnostic::new(
                        "module",
                        stmt.span,
                        format!("Circular import detected: {}", chain.join(" -> ")),
                    )
                    .with_file(&display_name)
                    .with_hint("Break the dependency cycle between modules");
                    return Err(diag.format_with_source(&source));
                }

                let loaded_dep_id = self.load_module_with_source(
                    dep_id,
                    dep_display,
                    dep_source,
                    dep_dir.as_deref(),
                )?;
                imports.insert(alias.clone(), loaded_dep_id);
            }
        }

        self.loading_stack.pop();
        self.modules.insert(
            id.clone(),
            LoadedModule {
                id: id.clone(),
                display_name,
                source,
                stmts,
                imports,
            },
        );
        self.load_order.push(id.clone());
        Ok(id)
    }

    fn resolve_and_read(
        &self,
        raw_path: &str,
        is_file_path: bool,
        base_dir: Option<&Path>,
        span: Span,
        importer_name: &str,
        importer_source: &str,
    ) -> Result<(String, String, String, Option<PathBuf>), String> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        let dir = base_dir.unwrap_or_else(|| Path::new(""));

        if is_file_path {
            let p = Path::new(raw_path);
            let joined = if p.is_absolute() {
                p.to_path_buf()
            } else {
                dir.join(p)
            };
            candidates.push(joined.clone());
            if joined.extension().is_none() {
                let mut with_ext = joined;
                with_ext.set_extension("pol");
                candidates.push(with_ext);
            }
        } else {
            candidates.push(dir.join(format!("{}.pol", raw_path)));
            candidates.push(dir.join(raw_path).join("mod.pol"));
        }

        for cand in &candidates {
            let norm = normalize_path(cand);
            if let Some(vsrc) = self.virtual_files.get(&norm) {
                let dep_dir = cand
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf());
                return Ok((norm.clone(), norm, vsrc.clone(), dep_dir));
            }
            if cand.exists() && cand.is_file() {
                let src = fs::read_to_string(cand).map_err(|e| {
                    Diagnostic::new(
                        "module",
                        span,
                        format!("Failed to read module '{}': {}", cand.display(), e),
                    )
                    .with_file(importer_name)
                    .format_with_source(importer_source)
                })?;
                let canon = fs::canonicalize(cand)
                    .map(|cp| cp.to_string_lossy().into_owned())
                    .unwrap_or(norm);
                let dep_dir = cand.parent().map(|p| p.to_path_buf());
                return Ok((canon, cand.to_string_lossy().into_owned(), src, dep_dir));
            }
        }

        let searched: Vec<String> = candidates.iter().map(|c| normalize_path(c)).collect();
        let diag = Diagnostic::new(
            "module",
            span,
            format!("Cannot find module '{}'", raw_path),
        )
        .with_file(importer_name)
        .with_hint(format!("Searched paths: {}", searched.join(", ")));
        Err(diag.format_with_source(importer_source))
    }
}

fn normalize_path_str(raw: &str) -> String {
    normalize_path(Path::new(raw))
}

fn normalize_path(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    let is_abs = path.is_absolute();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !parts.is_empty() && parts.last().map(|s| s.as_str()) != Some("..") {
                    parts.pop();
                } else if !is_abs {
                    parts.push("..".to_string());
                }
            }
            Component::Normal(os) => {
                parts.push(os.to_string_lossy().into_owned());
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    let joined = parts.join("/");
    if is_abs {
        format!("/{}", joined)
    } else {
        joined
    }
}
