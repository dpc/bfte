# bfte-module

Core module system interface and effect framework for BFTE applications.

## Overview

This crate defines the module system that enables BFTE to support different applications and use cases. Modules provide all application-level functionality, while the consensus system ensures agreement on module decisions across the federation.

## Key Concepts

### Module Interface
- **`IModule` Trait** - standardized interface all modules must implement
- **Module Configuration** - defines module kind and version
- **Database Isolation** - each module gets its own database namespace

### Effect System
- **`CItemEffect`** - typed messages for inter-module communication
- **Effect Processing** - modules produce effects that are processed by the node
- **Consensus Integration** - effects can trigger consensus decisions
- **Type Safety** - compile-time guarantees for effect handling

### Module Types
- **Core Modules** - essential system functionality (consensus control, meta)
- **Application Modules** - business logic and features specific to use cases
- **Singleton vs Multiple** - some modules can have multiple instances

## Architecture

### Database Integration
- **Isolated Namespaces** - each module instance gets isolated database access
- **Transactional** - module operations participate in consensus transactions
- **Persistent** - module state survives node restarts
- **Versioned** - database schema can evolve with module versions

## Usage

Modules implement the `IModule` trait and define their effects:

```rust
impl IModule for MyModule {
    async fn process_consensus_item(&self, item: CItem) -> Vec<CItemEffect> {
        // Process consensus decisions and return effects
    }
    
    async fn apply_effects(&self, effects: Vec<CItemEffect>) -> Result<()> {
        // Apply effects from other modules
    }
}
```


Module construction is handled by the `IModuleInit` trait.
