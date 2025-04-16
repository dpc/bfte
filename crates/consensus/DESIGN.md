

BFTE implements a refolmulation of the original Simplex protocol.
It is (should be) functionally the same, but is just slightly
differently specified and implemented.

For more detailed information about it, see:

* <https://simplex.blog/>
* <https://eprint.iacr.org/2023/463.pdf>
* <https://github.com/ava-labs/Simplex>
* <https://github.com/ava-labs/Simplex/issues/152>
* <https://github.com/ava-labs/Simplex/issues/150#issuecomment-2830906793>

The terminology has been changed slightly, to make it easier
to understand. Notably "dummies" are not called "dummy blocks",
as it's just confusing, and they are not part of the blockchain
anyway.

The communication pattern was reversed from "broadcasting" messages
("push"-based communication) to peers requesting
updates from other peers ("pull"-based communication).
This makes the implementation simpler, as each peer only needs to
track what they themselves are missing (which they naturally do),
instead of what they already delivered to other peers.

Unlike in other consensus implementations the author studied
before starting this implementation, incoming events are not stored
in a write-ahead log, and the state of the consensus is maintained
and persisted at all times in a database (key-value store).

## Overview

Peers are trying to agree on a blockchain, consisting of blocks,
each extending and committing to the previous one.

Consensus happens in rounds. Every round can, and typically should,
produce a single block extending the current blockchain.


Every round has a single random-like but deterministic leader peer,
which is the only peer allowed to propose the block. Other peers can
vote to approve it.

A round might produce a "block", or a "dummy". Dummies are
basically empty non-blocks and not included into the blockchain,
but they do advance the round number.

A block that gathered `threshold` amount of votes, is considered
"notarized".

#### Round sequence

Each round peer makes two requests (blocking, retrying) to all other peers:

* request for votes/proposal for the current round
* request for new notarized blocks (higher round than one they already have),
  or a notarized dummy for the current round

At the same time peer starts a round timeout.

A proposal/vote is a block signed by the peer. Round leader can propose a block,
other peers can sign the same block that leader proposed.

If a peer is a leader, it will immediately produce a block proposal at the start of
the round. Correct leader's proposal is valid and extending their current valid
view of the blockchain.

A correct peer will vote only on a first proposal from the leader, and only if
it extends it's current view of the blockchain.

The peer will abandon its current already notarized view of the blockchain,
*only* if it sees a notarized block/blockchain with a round higher than the
one it already has, as it means other peers have already abandoned it too.

*Switching to a notarized block might revert an already notarized block to a dummy,
but by definition there can never be two alternative non-dummy notarized blocks
at the same round. (Assuming number of correct peers.)*

*Since notarized blocks are abandoned only for higher round notarized blocks,
and higher round notarized blocks could only be produced by proposing blocks
that extending notarized views of at least `threshold` peers, it is evident
that the algorithm is always making progress and peers converge on a common
shared view.*

If a peer is unable to collect a notarization for the proposal before timeout
passes, they vote dummy. Once a peer collects a notarized block or a notarized
dummy, they consider the round finished and move to the next one.

At all times (even between rounds) a peer queries other peers for finalization
vote. Finalization vote is basically round past highest notarized block known
to the peer. Once at least a `threshold` amount of peers votes that certain round,
it is considered final, as there can be no alternative blocks produced that
would not strictly extend it, making it irreversible. Once a block is finalized
it can be made available to the application layer.

## Improvements

BFTE implements some improvements to Simplex consensus.

Peer member set can change via application level voting. This is achieved
by delaying changes to the consensus a fixed amount of rounds and maintaining
a "schedule". This ensures that consensus changes are final for all the peers
before they are applied.

As an additional layer of protection, round timeouts are increased exponentially
proportional to the number of rounds not yet finalized. This ensures that
in case of networking issues etc. the consensus slows down, giving peers more
time to request data from each other, and avoids exceeding the delay used
in the consensus change schedule.

To avoid producing round and blocks without any activity, the round timeout
is delayed until either the peer itself has pending consensus items,
or enough dummy votes from other peers, or just the leader itself were
already collected.


