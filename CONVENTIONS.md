# Engineering conventions

## Conventions

> When writing code, follow these conventions


- Do NOT add comments explaining what is each line/expression doing.
- Try to keep the code uniform, and follow the style of the existing code.
- Always use standalone Rust modules, avoid inline `mod`s
- Don't change anything without a good reason.
- Always prefer "lower than" or "lower than or equal" operators over "greater than" and "greater than or equal" ones.

## Documentation

- **Every Rust crate must have a README.md file** at its root directory containing:
  - A summary of what the crate does
  - High-level explanation of how it works
  - Key concepts and architecture
- **Include README.md as Rust documentation** by adding `#![doc = include_str!("../README.md")]` at the top of `lib.rs`
- **Update README.md when making significant changes** to maintain accuracy
- Keep documentation concise but informative for both users and developers

## Project structure

Most notable directories:

- `crates/` - all the project Rust modules (crates)
  - `crates/consensus` - Simplex algorithm implementation, deterministic and side-effect-free
  - `crates/node ` - implementation of BFTE node, driving communication with other peers and consensus changes
  - `crates/node-ui-axum` - web UI for the node administrator
  - `crates/util-*` - small utility crates with functionality described in the name
  - `crates/db` - wrapper around `redb-bincode`/`redb` database
  - `crates/derive-secret` - deterministic, hierarchical secret derivation

