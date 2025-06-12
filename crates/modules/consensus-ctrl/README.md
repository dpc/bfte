# bfte-module-consensus-ctrl

Core consensus control module that manages BFTE federation membership and module lifecycle.

## Overview

This is the foundational module present in every BFTE federation. It handles peer management, module activation, and consensus parameter changes. All federations start with this module enabled as it provides essential governance functionality.

## Key Responsibilities

### Peer Set Management
- **Add Peer Voting** - coordinate addition of new federation members
- **Remove Peer Voting** - coordinate removal of existing members  
- **Membership Consensus** - ensure all changes go through Byzantine fault tolerant agreement

### Module Lifecycle
- **Module Registration** - track which modules are active in the federation
- **Version Management** - coordinate module version upgrades
- **Dependency Resolution** - (TBD.) ensure module compatibility and ordering

### Consensus Parameters

- **Parameter Scheduling** - coordinate changes to core consensus settings
- **Network Configuration** - coordinate networking and communication settings

## Architecture

### Effects System

The module produces several types of consensus effects:

- `AddPeerEffect` - signals addition of new federation member
- `RemovePeerEffect` - signals removal of existing member
- `AddModuleEffect` - signals activation of new module type
- `ModuleVersionUpgradeEffect` - signals module version change
- `ConsensusParamsChange` - signals core parameter updates

