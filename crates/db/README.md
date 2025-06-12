# bfte-db

Database abstraction layer providing transactional key-value storage for BFTE.

## Overview

This crate provides a high-level interface over the `redb-bincode` database, offering type-safe, transactional key-value storage with automatic serialization. It serves as the foundation for all persistent state in BFTE.

## Architecture

### Database Transaction Context

The crate provides transaction contexts that wrap database operations:
- **ReadTransaction** - for read-only database access
- **WriteTransactionCtx** - for transactional updates
  - Notably supports emitting side-effects after transaction is successfully committed, using `on_commit` method
- **Error Handling** - comprehensive error types and recovery

### Table Management
Tables are defined using the `def_table!` macro:
```rust
def_table! {
    /// Table documentation
    table_name: KeyType => ValueType
}
```
