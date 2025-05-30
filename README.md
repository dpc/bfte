# BTF Engine

BTFE is a BFT consensus engine. A permissioned p2p blockchain to
drive BFT applications in federated setup, including financial (Bitcoin) domains.

## Introduction

It's fair to say BFTE in its current form is a research on alternative
implementation of a Fedimint-like system, and this project aims at the very similar *very-long*
term goals as [Fedimint][fedimint], but having benefit of years of extra experience, and allowing
focus on parts and goals that the author (dpc) finds most promising and interesting.

[fedimint]:  http://github.com/fedimint/fedimint

BFTE steals as much as possible from Fedimint, while doing things differently
where it seems to make sense.

Fedimint started as a Bitcoin Ecash Mint solution, with a side-goal/need
of building a general purpose consensus engine. BFTE stars with
a goal of being a good general purpose consensus engine first, with an ability
to support Bitcoin/cryptography applications like Ecash Mints "maybe one
day".

Things like Bitcoin and Lightning Network support are a huge time sinks,
with large and complex integrations and ever evolving ecosystem, with a huge
expectations on stability and robustness.

BFTE can just let Fedimint chart these difficult waters, while only
worrying about the general architecture eventually be able to support
what Fedimint can already do, while focusing on less mission-critical things,
like:

* CI system coordination,
* review systems,
* etc.

Similarly, Fedimint's ambition was always reaching broad end user appeal.
This requires a lot of effort: building end user clients including web and
mobile apps, cross-platform support, interoperability, API stability, backward
compatibility etc.

By ignoring all these ambitions, BFTE can focus first on honing the primary
goal: becoming good general purpose modular consensus engine.

## Links

* [BFTE Radicle site](https://app.radicle.xyz/nodes/radicle.dpc.pw/rad:zii8qFzZhN3vigh8BuxGCuEEp6z4) -
  BFTE uses [Radicle][radicle] as primary distributed code collaboration platform.
* [BFTE design document](./README.design.md)
* [BFTE consensus README (Simplex BFT implementation)](/crates/consensus/README.md)

[radicle]: https://radicle.xyz

## License

MPLE is licensed under MIT.
