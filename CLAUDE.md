# AI Assistant Guide for flightrecorder Development

**Version:** 1.0
**Last Updated:** 2026-01-13
**Purpose:** Guidelines for AI assistants working with the flightrecorder code base

## About This Document

This document provides essential guidance for AI assistants (like Claude Code) when working with the flightrecorder codebase. It covers project-specific conventions, patterns, and workflows.

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

Note: Depending upon the systesm, `assets/ai/ai-rust` may be a symlink; if so, you will need to look in `assets/ai/ai-rust/` (note the final slash). Depending upon the computer you are running on, the actual dir may be at `~/lab/oxur/ai-rust`, `~/lab/oxur/ai-rust-skill`, etc.

**For flightrecorder-Specific Topics:**

- **This file (CLAUDE.md)** - Project structure, ODDs, workflows, Oxur patterns
- **`assets/ai/CLAUDE-CODE-COVERAGE.md`** - Comprehensive test coverage guide
- **`README.md`** - High-level project overview
- **`crates/design/docs/01-draft/0001-TBD-placeholder.md`** - flightrecorder Implementation Guide
- **`crates/design/docs/05-active/0004-TBD-placeholder.md`** - flightrecorder Project Plan
- **`crates/design/docs/05-active/0005-TBD-placeholder.md`** - flightrecorder Implementation Stages

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

Core dependencies are managed at the workspace level. See `Cargo.toml` in the workspace root for the dependency list.

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

- [ ] Happy path (normal execution)
- [ ] Type errors (wrong operand types)
- [ ] Edge cases (empty, zero, boundary values)
- [ ] Error messages include spans
