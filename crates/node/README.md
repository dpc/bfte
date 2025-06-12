# bfte-node

The main BFTE node implementation that orchestrates consensus, networking, and application modules.

## Overview

This crate implements the complete BFTE node that runs the consensus algorithm, manages peer-to-peer communication, and coordinates with application modules. It serves as the central coordinator between all system components.

## Key Components

### Node Core
- **Consensus Integration** - drives the consensus algorithm and processes decisions
- **P2P Networking** - handles communication with other federation peers using IROH
- **Module Management** - loads and coordinates application modules
- **Database Management** - provides persistent storage for all components

### Networking Layer
- **RPC System** - custom RPC implementation over IROH for peer communication
- **Pull-based Protocol** - peers request data rather than broadcasting
- **Connection Management** - maintains connections to federation peers
- **Message Routing** - routes consensus and application messages appropriately

### Module Integration
- **Module Loading** - dynamically loads and initializes modules based on configuration
- **Effect Processing** - handles inter-module communication through effects
- **Database Isolation** - provides each module with isolated database namespaces
- **Lifecycle Management** - manages module startup, shutdown, and updates
