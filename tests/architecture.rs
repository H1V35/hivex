//! Domain dependencies are checked in addition to Rust's private interfaces.
use std::path::{Path, PathBuf};
use syn::{
  spanned::Spanned,
  visit::{self, Visit},
};

fn allowed(from: &str, to: &str) -> bool {
  if from == to || ["error", "compatibility"].contains(&to) {
    return true;
  }
  let dependencies: &[&str] = match from {
    "" | "cli" => &[
      "cli",
      "documents",
      "knowledge",
      "work",
      "execution",
      "integrations",
      "consultation",
      "review",
      "foundation",
    ],
    "documents" | "foundation" | "error" | "compatibility" => &[],
    "knowledge" | "review" => &["documents", "work", "execution", "knowledge"],
    "work" => &["documents", "knowledge", "execution"],
    "execution" => &["documents", "knowledge", "work"],
    "consultation" => &["documents", "knowledge", "work", "execution", "review"],
    "integrations" => &["execution"],
    _ => return false,
  };
  dependencies.contains(&to)
}

fn test_only(attributes: &[syn::Attribute]) -> bool {
  attributes.iter().any(|attribute| {
    attribute.path().is_ident("test")
      || (attribute.path().is_ident("cfg")
        && attribute
          .parse_args::<syn::Path>()
          .is_ok_and(|path| path.is_ident("test")))
  })
}

struct Boundaries {
  module: Vec<String>,
  directory: PathBuf,
  path: PathBuf,
  issues: Vec<String>,
}

impl Boundaries {
  fn macro_paths(&mut self, tokens: proc_macro2::TokenStream) {
    use proc_macro2::TokenTree;
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    let mut index = 0;
    while index < tokens.len() {
      let token = &tokens[index];
      index += 1;
      if let TokenTree::Group(group) = token {
        self.macro_paths(group.stream());
      }
      let TokenTree::Ident(root) = token else {
        continue;
      };
      if !["crate", "super", "self"].contains(&root.to_string().as_str()) {
        continue;
      }
      let mut path = vec![root.to_string()];
      while let [
        TokenTree::Punct(a),
        TokenTree::Punct(b),
        TokenTree::Ident(name),
        ..,
      ] = &tokens[index..]
      {
        if a.as_char() != ':' || b.as_char() != ':' {
          break;
        }
        path.push(name.to_string());
        index += 3;
      }
      if path.len() > 1 {
        self.check(&path, root.span());
      }
    }
  }

  fn check(&mut self, segments: &[String], span: proc_macro2::Span) {
    let Some(first) = segments.first() else {
      return;
    };
    let mut resolved = self.module.clone();
    let mut count = 0;
    match first.as_str() {
      "crate" => {
        resolved.clear();
        count = 1;
      }
      "self" => count = 1,
      "super" => {
        for segment in segments.iter().take_while(|segment| *segment == "super") {
          let _ = segment;
          resolved.pop();
          count += 1;
        }
      }
      _ => return,
    }
    resolved.extend_from_slice(&segments[count..]);
    let from = self.module.first().map_or("", String::as_str);
    let Some(to) = resolved.first() else {
      self.issue(span, "do not alias or glob-import the crate root");
      return;
    };
    if !allowed(from, to) {
      self.issue(span, &format!("forbidden domain dependency {from} -> {to}"));
    }
  }

  fn issue(&mut self, span: proc_macro2::Span, message: &str) {
    self.issues.push(format!(
      "{}:{}: {message}",
      self.path.display(),
      span.start().line
    ));
  }

  fn imports(&mut self, tree: &syn::UseTree, prefix: &[String]) {
    let mut path = prefix.to_vec();
    match tree {
      syn::UseTree::Path(tree) => {
        path.push(tree.ident.to_string());
        self.imports(&tree.tree, &path);
      }
      syn::UseTree::Group(group) => {
        for tree in &group.items {
          self.imports(tree, &path);
        }
      }
      syn::UseTree::Name(name) => {
        if name.ident != "self" {
          path.push(name.ident.to_string());
        }
        self.check(&path, tree.span());
      }
      syn::UseTree::Rename(rename) => {
        if rename.ident != "self" {
          path.push(rename.ident.to_string());
        }
        self.check(&path, tree.span());
      }
      syn::UseTree::Glob(_) => self.check(&path, tree.span()),
    }
  }
}

impl<'ast> Visit<'ast> for Boundaries {
  fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
    self.macro_paths(invocation.tokens.clone());
  }

  fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}

  fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
    if test_only(&module.attrs) {
      return;
    }
    if module
      .attrs
      .iter()
      .any(|attribute| attribute.path().is_ident("path"))
    {
      self.issue(
        module.span(),
        "production modules must use the conventional source path",
      );
      return;
    }
    if !matches!(module.vis, syn::Visibility::Inherited) {
      self.issue(
        module.span(),
        "domain submodules stay private; expose selected items at the domain root",
      );
    }
    let name = module.ident.to_string();
    let mut child = Self {
      module: self.module.iter().cloned().chain([name.clone()]).collect(),
      directory: self.directory.join(&name),
      path: self.path.clone(),
      issues: Vec::new(),
    };
    if let Some((_, items)) = &module.content {
      for item in items {
        child.visit_item(item);
      }
    } else {
      let flat = self.directory.join(format!("{name}.rs"));
      child.path = if flat.is_file() {
        flat
      } else {
        child.directory.join("mod.rs")
      };
      let source = std::fs::read_to_string(&child.path).expect("read declared production module");
      child.visit_file(&syn::parse_file(&source).expect("parse declared production module"));
    }
    self.issues.extend(child.issues);
  }

  fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
    if !test_only(&function.attrs) {
      visit::visit_item_fn(self, function);
    }
  }

  fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
    self.imports(&item.tree, &[]);
  }

  fn visit_path(&mut self, path: &'ast syn::Path) {
    self.check(
      &path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>(),
      path.span(),
    );
    visit::visit_path(self, path);
  }
}

#[test]
fn production_domain_dependencies() {
  let path = Path::new("src/main.rs");
  let mut boundary = Boundaries {
    module: Vec::new(),
    directory: PathBuf::from("src"),
    path: path.into(),
    issues: Vec::new(),
  };
  boundary.visit_file(&syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap());
  assert!(boundary.issues.is_empty(), "{}", boundary.issues.join("\n"));
}

#[test]
fn dependency_gate_covers_imports_qualified_paths_and_test_boundaries() {
  let mut boundary = Boundaries {
    module: vec!["documents".into()],
    directory: PathBuf::new(),
    path: "example.rs".into(),
    issues: Vec::new(),
  };
  let source = r#"
    use crate::knowledge as graph;
    use super::{review, error::Result};
    use crate as root;
    pub mod internals {}
    #[path = "alternate.rs"] mod redirected;
    fn violation() { crate::execution::run(); json!({"value": crate::work::read()}); }
    #[cfg(test)] mod tests { use crate::knowledge; }
  "#;
  boundary.visit_file(&syn::parse_file(source).unwrap());
  assert_eq!(boundary.issues.len(), 7);
  boundary.issues.clear();
  boundary.module = vec!["knowledge".into()];
  boundary.visit_file(
    &syn::parse_file("use crate::documents::Document; fn valid() { crate::work::save(); }")
      .unwrap(),
  );
  assert!(boundary.issues.is_empty());
}
