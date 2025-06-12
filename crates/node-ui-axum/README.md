# bfte-node-ui-axum

Web-based administration interface for BFTE nodes using Axum, Maud, and web-client-side frameworks.

## Overview

This crate provides a simple web UI for BFTE node administration and monitoring. It offers real-time status monitoring, federation management, module administration, and consensus history exploration through a web interface.

## Key Features

### Real-time Monitoring
- **Consensus Status** - live updates of current round, finality, and node state
- **Peer Information** - display of federation peers and connection status  
- **Database Status** - monitoring of storage backend (persistent vs ephemeral)
- **Performance Metrics** - consensus timing and throughput information

### Federation Management
- **Initialization** - create new federations with initial peer configuration
- **Invitation System** - generate and manage invite codes for new peers
- **Login System** - secure access with configurable password authentication

### Module Administration
- **Module Overview** - display active modules and their configurations
- **Consensus Control** - manage peer set and vote on membership changes
- **Meta Module** - key-value consensus for federation metadata
- **Module Lifecycle** - add new modules and manage versions

### Consensus Explorer
- **History Browser** - view last 1000 consensus rounds with detailed information
- **Block Analysis** - inspect block headers, payload sizes, and signatures
- **Dummy Round Tracking** - identify consensus rounds without blocks
- **Peer Signatures** - detailed view of which peers signed each round

## Technology Stack

### Frontend Technologies
- **Maud** - type-safe HTML templating for Rust
- **Datastar** - reactive frontend framework for real-time updates
- **Alpine.js** - lightweight JavaScript framework for interactivity
- **PicoCSS** - minimal CSS framework for clean styling

### Backend Architecture
- **Axum** - async web framework with excellent performance
- **Tower** - middleware ecosystem for HTTP services
- **Server-Sent Events** - real-time updates without WebSocket complexity
- **Session Management** - secure login state persistence

## Architecture

### Route Organization
- **Status Routes** - consensus overview and real-time monitoring
- **Module Routes** - per-module administration interfaces
- **Auth Routes** - login, password management, and security
- **Explorer Routes** - consensus history and analysis tools
