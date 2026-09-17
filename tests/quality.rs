//! Source quality gate. Mozilla owns metrics; syn validates current Rust and source boundaries.
use proc_macro2::Span;
use rust_code_analysis::{FuncSpace, ParserTrait, RustParser, SpaceKind, metrics};
use std::path::Path;
use syn::{
  spanned::Spanned,
  visit::{self, Visit},
};

#[derive(Debug)]
struct Measurement {
  line: usize,
  cognitive: f64,
  cyclomatic: f64,
}

fn measurements(source: &str, line: usize) -> Result<Vec<Measurement>, String> {
  let path = Path::new("source.rs");
  let parser = RustParser::new(source.as_bytes().to_vec(), path, None);
  if parser.get_root().has_error() {
    return Err("Mozilla cannot parse this function; no metrics accepted".into());
  }
  let space = metrics(&parser, path).ok_or("Mozilla returned no metrics")?;
  let mut output = Vec::new();
  collect_metrics(&space, line, &mut output);
  if output.is_empty() {
    return Err("Mozilla found no function or closure".into());
  }
  Ok(output)
}

fn collect_metrics(space: &FuncSpace, offset: usize, output: &mut Vec<Measurement>) {
  if space.kind == SpaceKind::Function {
    output.push(Measurement {
      line: space.start_line + offset,
      cognitive: space.metrics.cognitive.cognitive(),
      cyclomatic: space.metrics.cyclomatic.cyclomatic(),
    });
  }
  for child in &space.spaces {
    collect_metrics(child, offset, output);
  }
}

struct Syntax<'a> {
  source: &'a str,
  issues: Vec<String>,
  blocks: usize,
  in_function: bool,
  measured: usize,
}

impl Syntax<'_> {
  fn macro_flow(&mut self, tokens: proc_macro2::TokenStream) {
    let mut previous_else = false;
    for token in tokens {
      if let proc_macro2::TokenTree::Group(group) = &token {
        self.macro_flow(group.stream());
      }
      if let proc_macro2::TokenTree::Ident(name) = &token {
        if previous_else && name == "if" {
          self.issue(name.span(), "else-if chain in macro input");
        }
        previous_else = name == "else";
      } else {
        previous_else = false;
      }
    }
  }

  fn issue(&mut self, span: Span, message: &str) {
    self
      .issues
      .push(format!("{}: {message}", span.start().line));
  }

  fn measure(&mut self, span: Span, closure: bool) {
    let source = &self.source[span.byte_range()];
    // An initializer closure is an expression, so supply only its item context.
    let wrapped = format!("const _: _ = {source};");
    let source = if closure { &wrapped } else { source };
    let values = match measurements(source, span.start().line - 1) {
      Ok(values) => values,
      Err(error) => {
        self.issue(span, &error);
        return;
      }
    };
    self.measured += values.len();
    for value in values {
      if !value.cognitive.is_finite()
        || !value.cyclomatic.is_finite()
        || value.cognitive > 15.0
        || value.cyclomatic > 20.0
      {
        self.issues.push(format!(
          "{}: cognitive={} (max 15), cyclomatic={} (max 20)",
          value.line, value.cognitive, value.cyclomatic
        ));
      }
    }
  }

  fn signature(&mut self, signature: &syn::Signature) {
    if signature.inputs.len() > 4 {
      self.issue(
        signature.span(),
        "more than four parameters, including self",
      );
    }
  }

  fn body(&mut self, block: &syn::Block) {
    let prior_blocks = self.blocks;
    let prior_function = self.in_function;
    self.blocks = 0;
    self.in_function = true;
    self.visit_block(block);
    self.blocks = prior_blocks;
    self.in_function = prior_function;
  }
}

impl<'ast> Visit<'ast> for Syntax<'_> {
  fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
    self.macro_flow(invocation.tokens.clone());
  }

  fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
    if !self.in_function {
      self.measure(function.span(), false);
    }
    self.signature(&function.sig);
    self.body(&function.block);
  }

  fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
    if !self.in_function {
      self.measure(function.span(), false);
    }
    self.signature(&function.sig);
    self.body(&function.block);
  }

  fn visit_trait_item_fn(&mut self, function: &'ast syn::TraitItemFn) {
    self.signature(&function.sig);
    if let Some(block) = &function.default {
      if !self.in_function {
        self.measure(function.span(), false);
      }
      self.body(block);
    }
  }

  fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
    if closure.inputs.len() > 4 {
      self.issue(closure.span(), "more than four parameters in a closure");
    }
    if !self.in_function {
      self.measure(closure.span(), true);
    }
    let prior_blocks = self.blocks;
    let prior_function = self.in_function;
    self.blocks = usize::from(!matches!(*closure.body, syn::Expr::Block(_)));
    self.in_function = true;
    self.visit_expr(&closure.body);
    self.blocks = prior_blocks;
    self.in_function = prior_function;
  }

  fn visit_block(&mut self, block: &'ast syn::Block) {
    self.blocks += 1;
    if self.in_function && self.blocks > 4 {
      self.issue(
        block.span(),
        "more than three nested blocks inside the function or closure body",
      );
    }
    visit::visit_block(self, block);
    self.blocks -= 1;
  }

  fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
    if expression
      .else_branch
      .as_ref()
      .is_some_and(|(_, branch)| matches!(**branch, syn::Expr::If(_)))
    {
      self.issue(
        expression.span(),
        "else-if chain; use guards or a meaningful match",
      );
    }
    visit::visit_expr_if(self, expression);
  }
}

fn analyze(source: &str) -> Result<Syntax<'_>, syn::Error> {
  let file = syn::parse_file(source)?;
  let mut syntax = Syntax {
    source,
    issues: Vec::new(),
    blocks: 0,
    in_function: false,
    measured: 0,
  };
  syntax.visit_file(&file);
  Ok(syntax)
}

fn inspect(path: &Path, issues: &mut Vec<String>) -> usize {
  if path.is_dir() {
    return std::fs::read_dir(path)
      .expect("read source directory")
      .map(|entry| inspect(&entry.expect("read source entry").path(), issues))
      .sum();
  }
  if path.extension().is_none_or(|extension| extension != "rs") {
    return 0;
  }
  let source = std::fs::read_to_string(path).expect("read Rust source");
  let syntax = analyze(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
  issues.extend(
    syntax
      .issues
      .into_iter()
      .map(|issue| format!("{}:{issue}", path.display())),
  );
  syntax.measured
}

#[test]
fn repository_quality() {
  let mut issues = Vec::new();
  let count: usize = ["src", "tools", "tests"]
    .iter()
    .map(|root| inspect(Path::new(root), &mut issues))
    .sum();
  assert!(count > 0, "no Rust functions measured");
  assert!(issues.is_empty(), "{}", issues.join("\n"));
  println!("Measured {count} functions/methods/closures with Mozilla rust-code-analysis 0.0.25");
}

#[test]
fn metrics_cover_current_rust_without_accepting_parser_errors() {
  let source = r#"
    unsafe extern "C" { fn external(a: i32, b: i32, c: i32, d: i32, e: i32); }
    fn free(value: Option<i32>) { let Some(x) = value else { return; }; if x > 0 && x < 5 { return; } }
    struct Example;
    impl Example { fn method(&self) { let _ = |x| { if x { 1 } else { 0 } }; } }
    trait Defaulted { fn defaulted(&self) { for _ in 0..2 {} } }
    const CLOSURE: fn(bool) -> i32 = |value| { if value { 1 } else { 0 } };
  "#;
  let checked = analyze(source).unwrap();
  assert!(checked.issues.is_empty(), "{:?}", checked.issues);
  assert_eq!(checked.measured, 5);
  assert!(analyze("fn broken( {").is_err());
  assert!(measurements("fn broken( {", 0).is_err());
  let unsupported =
    analyze("fn pointer(value: &i32) { let pointer = &raw const *value; }").unwrap();
  assert!(
    unsupported
      .issues
      .iter()
      .any(|issue| issue.contains("Mozilla cannot parse"))
  );
  let values = measurements("fn parent() { let _ = |x| { if x { 1 } else { 0 } }; }", 0).unwrap();
  assert_eq!(values.len(), 2);
  assert!(values[0].cognitive.abs() < f64::EPSILON);
  assert!((values[1].cyclomatic - 2.0).abs() < f64::EPSILON);
}

#[test]
fn thresholds_and_flow_rules_reject_violations() {
  for (count, accepted) in [(15, true), (16, false)] {
    let body = "if value { return; }".repeat(count);
    let source = format!("fn cognitive(value: bool) {{ {body} }}");
    assert_eq!(analyze(&source).unwrap().issues.is_empty(), accepted);
  }
  for (count, accepted) in [(19, true), (20, false)] {
    let body = "Some(())?;".repeat(count);
    let source = format!("fn cyclomatic() -> Option<()> {{ {body} Some(()) }}");
    assert_eq!(analyze(&source).unwrap().issues.is_empty(), accepted);
  }
  let repeated = "if value { return; }".repeat(20);
  let checked = analyze(&format!("fn complex(value: bool) {{ {repeated} }}"))
    .unwrap()
    .issues;
  assert!(checked.iter().any(|issue| issue.contains("cyclomatic=21")));
  assert!(checked.iter().any(|issue| issue.contains("cognitive=20")));
  let checked = analyze(
    "impl Example { fn method(&self,a:i32,b:i32,c:i32,d:i32) { if a>0 {} else if b>0 {} } }",
  )
  .unwrap();
  assert!(
    checked
      .issues
      .iter()
      .any(|issue| issue.contains("four parameters"))
  );
  assert!(checked.issues.iter().any(|issue| issue.contains("else-if")));
  let closure = analyze("fn closure_arity() { let closure = |a,b,c,d,e| {}; }").unwrap();
  assert!(
    closure
      .issues
      .iter()
      .any(|issue| issue.contains("four parameters"))
  );
  assert!(
    analyze("fn closure_arity() { let closure = |a,b,c,d| {}; }")
      .unwrap()
      .issues
      .is_empty()
  );
  for body in ["|| if true { { { {} } } }", "|| { if true { { { {} } } } }"] {
    let source = format!("fn closure_nesting() {{ let closure = {body}; }}");
    assert!(
      analyze(&source)
        .unwrap()
        .issues
        .iter()
        .any(|issue| issue.contains("three nested"))
    );
  }
  let checked = analyze("fn nested() { { { { {} } } } }").unwrap();
  assert!(
    checked
      .issues
      .iter()
      .any(|issue| issue.contains("three nested"))
  );
  let macros = analyze("fn macro_input(value: bool) { json!({\"answer\": if value { 1 } else if value { 2 } else { 0 }}); }").unwrap();
  assert!(macros.issues.iter().any(|issue| issue.contains("else-if")));
  assert!(
    analyze("impl Example { fn allowed(&self,a:i32,b:i32,c:i32) { { { {} } } } }")
      .unwrap()
      .issues
      .is_empty()
  );
}
