# AI Assistant Guide for Treebeard Development

**Version:** 1.0
**Last Updated:** 2026-01-11
**Purpose:** Guidelines for AI assistants working with the Treebeard interpreter

## About This Document

This document provides essential guidance for AI assistants (like Claude Code) when working with the Treebeard codebase. It covers project-specific conventions, patterns, and workflows.

### Quick Navigation

- [Project Overview](#project-overview)
- [Architecture](#architecture)
- [Workspace Structure](#workspace-structure)
- [Development Environment](#development-environment)
- [Code Conventions](#code-conventions)
- [Testing Requirements](#testing-requirements)
- [Common Workflows](#common-workflows)
- [Resources](#resources)

---

### Document Hierarchy

**For Rust Code Quality:**

1. **`assets/ai/ai-rust/skills/claude/SKILL.md`** - Advanced Rust programming skill (**use this**)
2. **`assets/ai/ai-rust/guides/*.md`** - Comprehensive Rust guidelines referenced by the skill
3. **`assets/ai/ai-rust/README.md`** - How to use `ai-rust`
4. **`assets/ai/ai-rust/guides/README.md`** - How to use the guides in `ai-rust`

Note: `assets/ai/ai-rust` is a symlink; you will need to look in `assets/ai/ai-rust/` (note the final slash). Depending upon the computer you are running on, the actual dir may be at `~/lab/oxur/ai-rust`, `~/lab/oxur/ai-rust-skill`, etc.

**For Treebeard-Specific Topics:**

- **This file (CLAUDE.md)** - Project structure, ODDs, workflows, Oxur patterns
- **`assets/ai/CLAUDE-CODE-COVERAGE.md`** - Comprehensive test coverage guide
- **`README.md`** - High-level project overview
- **`crates/design/docs/01-draft/0001-treebeard-implementation-guide-v3.md`** - Treebeard Implementation Guide
- **`crates/design/docs/05-active/0004-treebeard-project-plan.md`** - Treebeard Project Plan
- **`crates/design/docs/05-active/0005-treebeard-implementation-stages.md`** - Treebeard Implementation Stages

---

## Project Overview

### What is Treebeard?

Treebeard is a **tree-walking interpreter for Rust's `syn` AST**. It directly executes `syn` AST nodes by recursively traversing them—without invoking `rustc`. Any language frontend that can produce `syn` AST can use Treebeard for immediate execution.

**Key characteristics:**

- **Zero compilation delay** — Instant execution via AST interpretation
- **Hot code reload** — Redefine functions without restart
- **`rustc` escape hatch** — Compile hot paths to native code when needed
- **Frontend agnostic** — Works with any syntax that compiles to `syn` AST

### Design Principles

1. **Thin Layer** — Do interpretation, delegate everything else to Rust/rustc
2. **Late Binding** — Always look up functions at call time (enables hot reload)
3. **Incremental Coverage** — Implement `syn` types as needed, not all at once
4. **Helpful Errors** — Runtime errors should be clear and actionable
5. **Performance Awareness** — Track overhead, provide escape hatches

### Relationship to Oxur

Treebeard is the execution engine for Oxur (a Lisp dialect targeting Rust). However, Treebeard is **language-agnostic**—it only sees `syn` AST, not Oxur syntax. The `LanguageFrontend` trait defines the boundary:

```
Oxur Source → [Oxur Frontend] → syn AST → [Treebeard] → Value
Rust Source → [Rust Frontend] → syn AST → [Treebeard] → Value
```

---

## Architecture

### Core Components

Treebeard's architecture follows a phased implementation approach. The core interpreter is structured around these key components:

- **Value System** — Runtime value representation supporting all Rust types
- **Environment** — Variable and function bindings with scoped frame management
- **Evaluator** — Tree-walking interpreter for `syn` AST nodes
- **Error Handling** — Span-aware error reporting for helpful diagnostics
- **Ownership Tracking** — Runtime checks for move/borrow violations
- **Frontend Trait** — Language-agnostic abstraction for multiple syntax frontends
- **REPL** — Interactive development environment
- **Compilation Escape Hatch** — Dynamic compilation to native code for hot paths

The exact file structure will evolve during implementation. See the Project Plan and Implementation Stages documents for the phased development approach.

### Key Types

| Type | Purpose |
|------|---------|
| `Value` | Runtime value representation |
| `Environment` | Scoped variable bindings |
| `EvalContext` | Evaluation configuration |
| `EvalError` | Error with source span |
| `LanguageFrontend` | Frontend abstraction |

### Evaluation Flow

```rust
// 1. Frontend parses source to syn AST
let items: Vec<syn::Item> = frontend.parse(source)?;

// 2. Frontend expands macros (if applicable)
let expanded = frontend.expand_macros(items, &macro_env)?;

// 3. Treebeard evaluates the AST
let result = treebeard::eval_items(&expanded, &mut env, &ctx)?;
```

---

## Workspace Structure

The project is organized as a Cargo workspace:

```
treebeard/
├── Cargo.toml           # Workspace configuration
├── CLAUDE.md            # This file
├── README.md            # Project overview
├── Makefile             # Development targets
├── crates/
│   ├── treebeard/       # Core interpreter (implementation in progress)
│   └── design/          # Design documentation (ODDs managed by oxur-odm)
├── assets/
│   ├── ai/              # AI assistant guides and resources
│   └── images/          # Project assets
└── tests/               # Integration tests (to be added)
```

The crate structure will evolve during implementation following the phased approach outlined in the Project Plan. The `design/` crate uses `oxur-odm` (Oxur Design Document Manager) to organize design documents by status (draft, active, superseded, etc.).

---

## Development Environment

### Required Tools

```bash
# Rust toolchain (1.75+ stable)
rustup default stable
rustup component add rustfmt clippy

# Coverage tool
cargo install cargo-llvm-cov
```

### Makefile Targets

```bash
make build        # Build all crates
make test         # Run all tests
make lint         # Run clippy + rustfmt check
make format       # Format all code
make coverage     # Generate coverage report
make check        # build + lint + test
```

### Key Dependencies

Core dependencies are managed at the workspace level. Key dependencies include:

- **syn 2.0** — AST types with full features
- **quote, proc-macro2** — AST construction and manipulation
- **thiserror, anyhow** — Error handling
- **dashmap** — Concurrent data structures
- **tokio** — Async runtime (for REPL)
- **rustyline** — REPL line editing
- **oxur-odm** — Design document management

See `Cargo.toml` in the workspace root for the complete dependency list.

---

## Code Conventions

### Value Type Design

The `Value` enum is the heart of Treebeard. Follow these conventions:

```rust
// ✅ Good: Use Arc for heap types (enables sharing)
Value::String(Arc<String>)
Value::Vec(Arc<Vec<Value>>)

// ✅ Good: Inline small primitives
Value::I64(i64)
Value::Bool(bool)

// ❌ Bad: Box for heap types (no sharing)
Value::String(Box<String>)
```

### Environment Design

```rust
// ✅ Good: Flat scope with frame boundaries
impl Environment {
    pub fn push_frame(&mut self) { ... }
    pub fn pop_frame(&mut self) { ... }
    pub fn lookup(&self, name: &str) -> Option<&Value> { ... }
}

// ❌ Bad: Nested HashMap chain
struct Environment {
    bindings: HashMap<String, Value>,
    parent: Option<Box<Environment>>,
}
```

### Error Handling

Always include source spans for debugging:

```rust
// ✅ Good: Error with span
#[derive(Error, Debug)]
pub enum EvalError {
    #[error("undefined variable `{name}` at {span:?}")]
    UndefinedVariable { name: String, span: Span },

    #[error("type error: expected {expected}, got {got}")]
    TypeError { expected: String, got: String, span: Span },
}

// ❌ Bad: Error without location
#[derive(Error, Debug)]
pub enum EvalError {
    #[error("undefined variable")]
    UndefinedVariable,
}
```

### Evaluation Pattern

Use the `Evaluate` trait for consistency:

```rust
pub trait Evaluate {
    fn eval(&self, env: &mut Environment, ctx: &EvalContext) -> Result<Value, EvalError>;
}

impl Evaluate for syn::ExprBinary {
    fn eval(&self, env: &mut Environment, ctx: &EvalContext) -> Result<Value, EvalError> {
        let left = self.left.eval(env, ctx)?;
        let right = self.right.eval(env, ctx)?;
        apply_binary_op(&self.op, left, right, self.span())
    }
}
```

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Evaluation functions | `eval_*` | `eval_expr`, `eval_stmt` |
| Value constructors | `Value::*` or `Value::new_*` | `Value::string("hi")` |
| Type predicates | `is_*` | `value.is_integer()` |
| Type extractors | `as_*` | `value.as_i64()` |
| Error variants | `PascalCase` with context | `UndefinedVariable { name, span }` |

### Documentation

```rust
/// Evaluate a binary expression.
///
/// Supports arithmetic (+, -, *, /, %), comparison (==, !=, <, >, <=, >=),
/// logical (&&, ||), and bitwise (&, |, ^, <<, >>) operators.
///
/// # Errors
///
/// Returns `EvalError::TypeError` if operand types are incompatible.
/// Returns `EvalError::DivisionByZero` for division/modulo by zero.
pub fn eval_binary(expr: &syn::ExprBinary, env: &mut Environment) -> Result<Value, EvalError> {
    // ...
}
```

---

## Testing Requirements

### Coverage Target

**Target: 95% line coverage** for the core interpreter crate

See `assets/ai/CLAUDE-CODE-COVERAGE.md` for comprehensive testing guidelines.

```bash
make coverage  # Generates ASCII table coverage report
```

### Test Naming Convention

```rust
#[test]
fn test_<function>_<scenario>_<expected>() { }

// Examples:
fn test_eval_binary_add_integers_returns_sum() { }
fn test_eval_binary_divide_by_zero_returns_error() { }
fn test_environment_lookup_undefined_returns_none() { }
```

### Test Categories

**Unit tests** — Test individual functions in isolation:

```rust
#[test]
fn test_value_as_i64_from_i32() {
    let v = Value::I32(42);
    assert_eq!(v.as_i64(), Some(42));
}
```

**Evaluation tests** — Test expression evaluation:

```rust
#[test]
fn test_eval_if_true_branch() {
    let expr: syn::Expr = syn::parse_quote! { if true { 1 } else { 2 } };
    let mut env = Environment::new();
    let result = expr.eval(&mut env, &EvalContext::default()).unwrap();
    assert_eq!(result, Value::I64(1));
}
```

**Integration tests** — Test full programs:

```rust
#[test]
fn test_factorial_recursive() {
    let source = r#"
        fn factorial(n: i64) -> i64 {
            if n <= 1 { 1 } else { n * factorial(n - 1) }
        }
    "#;
    let result = eval_rust_source(source, "factorial(5)").unwrap();
    assert_eq!(result, Value::I64(120));
}
```

### What to Test

For each `syn` expression type implemented:

- [ ] Happy path (normal execution)
- [ ] Type errors (wrong operand types)
- [ ] Edge cases (empty, zero, boundary values)
- [ ] Error messages include spans

---

## Common Workflows

### Implementing a New `syn` Expression Type

1. **Check the `syn` docs** for the type's structure
2. **Add evaluation logic** in the evaluator
3. **Add tests** for all paths
4. **Update coverage** check
5. **Document** any limitations

Example for `syn::ExprRange`:

```rust
impl Evaluate for syn::ExprRange {
    fn eval(&self, env: &mut Environment, ctx: &EvalContext) -> Result<Value, EvalError> {
        let start = self.start.as_ref().map(|e| e.eval(env, ctx)).transpose()?;
        let end = self.end.as_ref().map(|e| e.eval(env, ctx)).transpose()?;

        // Create a range value...
        todo!("Range evaluation not yet implemented")
    }
}
```

### Adding a Built-in Function

1. **Define the function** following the built-in function signature pattern:

```rust
fn builtin_println(args: &[Value]) -> Result<Value, String> {
    for arg in args {
        print!("{}", arg);
    }
    println!();
    Ok(Value::Unit)
}
```

1. **Register it** in environment setup:

```rust
env.register_builtin("println", -1, builtin_println);  // -1 = variadic
```

1. **Add tests**

### Debugging Evaluation

Use the `EvalContext` for debugging:

```rust
let ctx = EvalContext {
    trace: true,  // Print each evaluation step
    max_call_depth: 100,
    ..Default::default()
};
```

---

## Resources

### Project Documentation

All design documents are managed using `oxur-odm` and organized by status in `crates/design/docs/`:

| Document | Purpose |
|----------|---------|
| `crates/design/docs/05-active/0004-treebeard-project-plan.md` | High-level phases and timeline (19 weeks) |
| `crates/design/docs/05-active/0005-treebeard-implementation-stages.md` | Detailed stage-by-stage breakdown |
| `crates/design/docs/01-draft/0001-treebeard-implementation-guide-v3.md` | Architecture and design decisions |
| `crates/design/docs/01-draft/0002-treebeard-fangorn-a-vision-document.md` | Future vision: Fangorn runtime |
| `assets/ai/CLAUDE-CODE-COVERAGE.md` | Comprehensive test coverage guide |

### Key References

- **[syn documentation](https://docs.rs/syn/)** — AST types we're interpreting
- **[Crafting Interpreters](https://craftinginterpreters.com/)** — Tree-walking interpreter patterns
- **[Rhai source](https://github.com/rhaiscript/rhai)** — Similar Rust interpreter for reference

### Oxur Integration

When Treebeard is used with Oxur:

- Oxur's AST Bridge produces `syn` AST from S-expressions
- Treebeard evaluates the AST
- The `OxurFrontend` implements `LanguageFrontend`

See the Oxur repository for frontend-specific conventions.

---

## Quick Reference Checklists

### Before Starting Work

- [ ] Read relevant implementation stage document from `crates/design/docs/05-active/0005-treebeard-implementation-stages.md`
- [ ] Review the phase goals in the Project Plan
- [ ] Understand which `syn` types are involved
- [ ] Check existing patterns in the codebase
- [ ] Know the success criteria
- [ ] Review `assets/ai/ai-rust/skills/claude/SKILL.md` and `assets/ai/ai-rust/guides/README.md` for which Rust best practices to apply for the planned work

### Before Submitting Changes

- [ ] All tests pass (`make test`)
- [ ] Coverage ≥ 95% for changed code (`make coverage`)
- [ ] Linting passes (`make lint`)
- [ ] Code formatted (`make format`)
- [ ] No compiler warnings or warnings of any type
- [ ] Public items have doc comments
- [ ] Error types include spans

### When Implementing Evaluation

- [ ] Handle all variants of the `syn` type
- [ ] Return appropriate `EvalError` for failures
- [ ] Include source span in errors
- [ ] Test happy path
- [ ] Test error paths
- [ ] Test edge cases
- [ ] Check for stack overflow risk (recursive evaluation)

---

## Summary

**Treebeard is a tree-walking interpreter for `syn` AST.**

Key things to remember:

1. **`Value`** is the universal runtime type — everything evaluates to a `Value`
2. **`Environment`** manages bindings — flat scope with frame boundaries
3. **`EvalError`** must include spans — users need to know *where* errors occur
4. **Test thoroughly** — 95%+ coverage, all paths tested
5. **Implement incrementally** — Not all `syn` types needed at once

**Document End**

**Last Updated:** 2026-01-11
**Version:** 1.0
