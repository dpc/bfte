# bfte-module-meta

Meta module providing key-value consensus for federation metadata

## Overview

The meta module enables federations to reach consensus on arbitrary key-value pairs, providing a foundation for storing federation metadata.

## Key Features

### Key-Value Consensus
- **Arbitrary Keys** - supports any 8-bit key identifier (0-255)
- **Arbitrary Values** - stores any binary data up to consensus-enforced limits
- **Atomic Updates** - each key-value update is a single consensus decision

## Use Cases

### Federation Metadata
- **Name and Description** - human-readable federation information
- **Contact Information** - administrator contact details
- **Website URLs** - federation website and documentation links
- **Feature Flags** - enable/disable client-side federation features through consensus
