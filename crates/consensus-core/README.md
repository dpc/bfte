# bfte-consensus-core

Core data structures and types for the BFTE Byzantine Fault Tolerant consensus engine.

## Overview

This crate provides the fundamental building blocks used throughout the BFTE consensus system:

- **Block structures** (`BlockHeader`, `BlockRound`, `BlockHash`, etc.)
- **Consensus parameters** (`ConsensusParams`, peer sets)
- **Cryptographic primitives** (signatures, hashes, peer identities)
- **Consensus items** (`CItem`) - the basic unit of consensus decisions
- **Module system types** (module IDs, kinds, versions)
- **Vote structures** for the consensus protocol

## Key Concepts

### Blocks and Rounds
- Each consensus round produces either a **real block** (with payload) or a **dummy**
- Blocks are identified by round number and contain references to previous blocks
- Block headers commit to payload hash, consensus parameters, and timestamps

### Consensus Parameters
- Define the current peer set and voting thresholds
- Can change over time through consensus decisions


## Architecture

This crate is purely data structures and contains no business logic. It serves as the interface contract between:
- The consensus algorithm implementation (`bfte-consensus`)
- Node networking and application layers (`bfte-node`)
- Individual modules (`bfte-module-*`)

All types implement appropriate serialization traits (`bincode`, `serde`) for network transmission and storage.
