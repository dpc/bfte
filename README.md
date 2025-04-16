# BTF Engine

BTFE is a BFT consensus engine. A permissioned p2p blockchain to
drive BFT applications in federated setup, including financial (Bitcoin) domains.

### BFTE vs Fedimint

It's fair to say BFTE in its current form is a research on alternative
implementation of a Fedimint-like system, and this project aims at the very similar *very-long*
term goals as [fedimint][fedimint], but having benefit of years of extra experience, and allowing
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

### Design (Ideas):

*Some of these might make more sense if you already understand Fedimint*.

In BFTE consensus is a central primitive, and near *everything* is part of the consensus history.

Adding and removing peers, changes to configuration like adding new modules
(applications), software upgrades, etc. are all implemented by consensus items agreed on
and published on a shared ledger.

Replicating, but not participating nodes are a first class primitive.
Anyone can run an additional consensus history replicating node, without necessarily participating
in consensus building.

A simple consensus algorithm (Simplex) is used to perfectly match
the needs of BFTE.

Simplicity is achieved through interactions between few well
thought general purpose pritmivites.

Trying to avoid non-consensus APIs and state.

Maintaining additional APIs, and client side state machines is just duplicating effort.
The mutable part of the consensus state is typically small anyway and
"clients" might maybe throw away the bulk data that they don't need anyway,
as they not validate the consensus.

Things like DKGs can just happen as a part of consensus. Mint module should support
multiple keysets, and generating new ones on demand.

Consensus items should be just module specific inputs/outputs, signed by the
node itself. This allows things like conditional consensus items.

Effect system as the only means of inter-module interactions. Modules processing consensus
items can produce typed and serialized "effects", which other modules that happen to understand
can act on.

### License

MPLE is licensed under MIT.
