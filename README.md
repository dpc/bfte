# BTF Engine

BTFE is a BFT consensus engine. A permissioned p2p blockchain to
drive BFT applications in federated setup, including financial (Bitcoin) domains.


## Radicle note

BFTE uses [Radicle][radicle] as a primary distributed code collaboration platform,
and Github repo is only a read-only mirror.

Head to [BFTE's Radicle site][bfte-radicle] for an up to date version.

## Introduction

BFTE in its current form is a research project on alternative
design/implementation of a Fedimint-like system. While the *very-long*
term goals and supported use-cases could be similar to [Fedimint][fedimint],
it has a luxury of not worrying about immediate practical application,
enabling focus on parts and goals that the author (dpc)
finds most promising and interesting.

[fedimint]:  http://github.com/fedimint/fedimint

BFTE steals as much as possible from Fedimint, while doing things differently
where it seems to make sense.

Fedimint started as a Bitcoin Ecash Mint solution, with a side-goal/need
of building a general purpose consensus engine. BFTE stars with
a goal of building as good as possible general purpose consensus engine first, with
an ability to support Bitcoin/cryptography applications like Ecash Mints "maybe one
day".

Things like Bitcoin and Lightning Network support are a big and difficult tasks
in itself, with large and complex integrations and ever evolving ecosystem, with a huge
expectations on stability and robustness.

BFTE can just let Fedimint chart these difficult waters, while only
worrying about the general architecture, which could eventually be able to support
what Fedimint can already do, while focusing on less mission-critical applications,
like:

* CI system coordination,
* review systems,
* etc.

basically - things that could benefit from a BFT Consensus.

Similarly, Fedimint's ambition was always reaching broad end user appeal.
This requires a lot of effort: building end user clients including web and
mobile apps, cross-platform support, interoperability, API stability, backward
compatibility etc.

By ignoring all these ambitions, BFTE can focus first on honing the primary
goal: becoming good general purpose modular consensus engine, and be ambitious
about other aspects of the design space and implementation.

## Status

As of last update the project has most of the core pieces in a working state.

Implemented:

* Simplex BFT consensu algorithm,
* web UI,
* consensus membership changes,
* core consensus and module consensus versioning and upgrades,
* consensus control module,
* metadata module (first simple example module),

## Running

BFTE is a standard Rust application. You can clone it and use `cargo` to build
and run it. Using the provided Nix Flake Dev Shell is recommend.

If you're a Nix user (which you should be), you can easily give it a try:

```
nix run git+https://radicle.dpc.pw/zii8qFzZhN3vigh8BuxGCuEEp6z4.git -- --help
```

eg.

```
> nix run git+https://radicle.dpc.pw/zii8qFzZhN3vigh8BuxGCuEEp6z4.git -- gen-secret > /tmp/secret
PeerId: nk2tb7xtkw5uqyxj65w0fhd3nqdhm7ztbs2uysxrgv3yfc5dbbn1


This mnemonic is irrecoverable if lost. Please make a back up before using it!

> nix run git+https://radicle.dpc.pw/zii8qFzZhN3vigh8BuxGCuEEp6z4.git -- run --secret-path /tmp/secret --data-dir /tmp/
2025-06-13T05:03:19.622758Z  INFO bfte::node: Opening redb database… path=/tmp/bfte.redb
2025-06-13T05:03:19.626664Z  INFO bfte::node: Iroh endpoint initialized endpoint=a182558e0be2a4c145ad60a5e92fd11356c2d441b43e7925e27b3964ca72ca43 bound_ipv4=0.0.0.0:35687 bound_ipv6=[::]:35688
2025-06-13T05:03:19.626707Z  WARN bfte::node: Temporary UI password pass=GYBJQPbFhk
2025-06-13T05:03:19.626858Z  INFO bfte::node: Waiting for consensus initialization via web UI
2025-06-13T05:03:19.626905Z  INFO bfte::node::ui: Starting web UI server... addr=[::1]:6910
```

## Links

* [BFTE Radicle site][bfte-radicle]
* [BFTE design document](./README.design.md)
* [BFTE consensus README (Simplex BFT implementation)](/crates/consensus/README.md)

[radicle]: https://radicle.xyz
[bfte-radicle]: https://app.radicle.xyz/nodes/radicle.dpc.pw/rad:zii8qFzZhN3vigh8BuxGCuEEp6z4 

## License

MPLE is licensed under MIT.
