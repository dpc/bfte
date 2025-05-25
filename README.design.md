# BFTE design

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

