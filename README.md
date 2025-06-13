# BTF Engine

BTFE is a BFT consensus engine. A permissioned p2p blockchain to
drive BFT applications in federated setup, including financial (Bitcoin) domains.


## Radicle note

BFTE uses [Radicle][radicle] as a primary distributed code collaboration platform,
and Github repo is only a read-only mirror.

Head to [BFTE's Radicle site][bfte-radicle] for an up to date version.

## Introduction

BFTE in its current form is a research project on alternative
design/implementation of a Fedimint-like system. While th *very-long*
term goals and supported use-cases could be similar to [Fedimint][fedimint],
it has a luxury of not worrying about immediate practical application,
and enabling focus on parts and goals that the author (dpc)
finds most promising and interesting.

[fedimint]:  http://github.com/fedimint/fedimint

BFTE steals as much as possible from Fedimint, while doing things differently
where it seems to make sense.

Fedimint started as a Bitcoin Ecash Mint solution, with a side-goal/need
of building a general purpose consensus engine. BFTE stars with
a goal of researching a good general purpose consensus engine first, with
an ability to support Bitcoin/cryptography applications like Ecash Mints "maybe one
day".

Things like Bitcoin and Lightning Network support are a big and difficult tasks
in itself, with large and complex integrations and ever evolving ecosystem, with a huge
expectations on stability and robustness.

BFTE can just let Fedimint chart these difficult waters, while only
worrying about the general architecture eventually be able to support
what Fedimint can already do, while focusing on less mission-critical applications,
like:

* CI system coordination,
* review systems,
* etc.

Basically - things that could benefit from a BFT Consensus.

Similarly, Fedimint's ambition was always reaching broad end user appeal.
This requires a lot of effort: building end user clients including web and
mobile apps, cross-platform support, interoperability, API stability, backward
compatibility etc.

By ignoring all these ambitions, BFTE can focus first on honing the primary
goal: becoming good general purpose modular consensus engine, and be ambitious
about other aspects of the design places and implementation.

## Status

As of last update the project has most of the core pieces in a working state:

* Simplex Consensu algorithm is implemented,
* has a usable web UI,
* Consensus Control Mode allows adding and removing peers from the consensus,
* Metadata Module is a first simple showcase module.

## Running

BFTE is a standard Rust application. You can clone it and use `cargo` to build
and run it. Using the provided Nix Flake Dev Shell is recommend.

If you're a Nix user (which you should be), you can easily give it a try:

```
nix run git+https://radicle.dpc.pw/zii8qFzZhN3vigh8BuxGCuEEp6z4.git
```

## Links

* [BFTE Radicle site][bfte-radicle]
* [BFTE design document](./README.design.md)
* [BFTE consensus README (Simplex BFT implementation)](/crates/consensus/README.md)

[radicle]: https://radicle.xyz
[bfte-radicle]: https://app.radicle.xyz/nodes/radicle.dpc.pw/rad:zii8qFzZhN3vigh8BuxGCuEEp6z4 

## License

MPLE is licensed under MIT.
