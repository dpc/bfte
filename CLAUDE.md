# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build and Test

- `just check` - Run cargo check on workspace
- `just build` - Build entire workspace
- `just test` - Run all tests (builds first) - at the current state of the project, don't use it

### Code Quality

- `just format` - Format Rust and Nix code
- `just lint` - Run git pre-commit hooks
- `just clippy` - Run clippy with deny warnings
- `just clippy-fix` - Auto-fix clippy issues


## Architecture Overview

**BFTE** is a Byzantine Fault Tolerant consensus engine implementing a modular architecture where consensus is the central primitive. Everything (peer management, configuration changes, module additions) goes through consensus.

### Key Components

#### Core Consensus (`crates/consensus/`)
- Implements Simplex BFT algorithm
- Deterministic and side-effect-free design
- Pull-based communication (peers request updates vs broadcasting)
- State maintained in database (no write-ahead log)

#### Node Infrastructure
- `crates/node/` - Main node driving consensus and P2P communication  
- `crates/node-app/` - Application layer processing consensus items
- `crates/node-ui-axum/` - Web administration UI using Axum + Maud + Datastar

#### Module System
- `crates/module/` - Module interface and effect system
- `crates/modules/core-consensus/` - Core consensus module
- Modules communicate exclusively through typed effects (`CItemEffect`)
- Each module gets isolated database namespace

#### Database Layer
- `crates/db/` - Wrapper around `redb-bincode`
- All state persisted in key-value store
- No external database dependencies

### Development Patterns

#### Module Development
- Modules implement `IModule` trait with standardized lifecycle
- Use `ModuleDatabase` for isolated state management
- Produce `CItemEffect`s for inter-module communication
- Follow effect system for all module interactions

#### Consensus Integration
- All significant changes go through consensus items
- Avoid non-consensus APIs and state
- Use pull-based communication patterns
- Maintain deterministic, side-effect-free logic in consensus layer

#### Code Conventions
- Always use standalone Rust modules (avoid inline `mod`s)
- No comments explaining individual lines/expressions
- Follow existing code style and uniformity
- Use workspace-based build with `cargo`

## Database and State Management

- Uses `redb` key-value store for all persistence
- Each module gets isolated database namespace via `ModuleDatabase`
- Consensus state maintained in database at all times
- No write-ahead logging - direct database state management

## Testing

- `cargo test` runs all workspace tests
- `crates/consensus-tests/` contains consensus algorithm tests  
- Integration tests in individual crate `tests/` directories
- Use `just test` which builds before testing

## Networking and P2P

- Uses `iroh` for IPFS-based networking
- Custom RPC system for peer communication
- Pull-based consensus communication (peers request vs broadcast)
- Federation join/invite system in `crates/invite/`
