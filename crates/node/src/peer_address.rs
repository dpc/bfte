use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use bfte_consensus_core::bincode::STD_BINCODE_CONFIG;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::signed::{Hashable, Signable, Signed};
use bfte_consensus_core::timestamp::Timestamp;
use bfte_db::Database;
use bfte_node_core::address::PeerAddress;
use bfte_util_bincode::decode_whole;
use bfte_util_db::def_table;
use bfte_util_db::random::get_random;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use bincode::{Decode, Encode};
use iroh_dpc_rpc::RpcExt as _;
use rand::Rng as _;
use snafu::{ResultExt as _, Whatever};
use tracing::{debug, instrument, warn};

use crate::Node;
use crate::rpc::{self, RPC_ID_PUSH_PEER_ADDR_UPDATE};

const LOG_TARGET: &str = "bfte::node::peer-addr";

#[derive(Clone, Debug, Encode, Decode)]
pub struct AddressUpdate {
    pub timestamp: Timestamp,
    pub peer_pubkey: PeerPubkey,
    pub addr: PeerAddress,
}

impl Hashable for AddressUpdate {}

impl Signable for AddressUpdate {
    const TAG: [u8; 4] = *b"adup";
}

impl fmt::Display for AddressUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "{}",
            data_encoding::BASE32_NOPAD.encode_display(
                &bincode::encode_to_vec(self, STD_BINCODE_CONFIG).expect("Can't fail")
            )
        ))
    }
}

impl FromStr for AddressUpdate {
    type Err = Whatever;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = data_encoding::BASE32_NOPAD
            .decode(s.as_bytes())
            .whatever_context("Failed to decode base32")?;
        decode_whole(&bytes, STD_BINCODE_CONFIG).whatever_context("Failed to decode bincode")
    }
}

def_table! {
    /// Table to track latest address updates for given `PeerPubkey`
    peer_addresses: PeerPubkey => Signed<AddressUpdate>
}

def_table! {
    peer_addresses_we_track: PeerPubkey => ()
}

def_table! {
    peer_addresses_we_need: PeerPubkey => ()
}

impl Node {
    #[instrument(
        name = "push_gossip"
        target = LOG_TARGET,
        skip_all,
    )]
    pub(crate) async fn run_push_gossip(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let Some((peer_pubkey, address_update)) = self.pick_push_gossip_pair().await else {
                continue;
            };

            if Some(peer_pubkey) == self.peer_pubkey {
                // we can't be updating our own self
                continue;
            }

            if peer_pubkey == address_update.peer_pubkey {
                // No need to update peer with own info
                continue;
            }

            debug!(
                target: LOG_TARGET,
                dst_peer = %peer_pubkey,
                src_peer = %address_update.peer_pubkey,
                "Sending peer address gossip"
            );
            if let Err(err) = self.send_push_gossip(peer_pubkey, address_update).await {
                debug!(
                    target: LOG_TARGET,
                    err = %err.fmt_compact(),
                    "Failed to push a peer address via gossip"
                );
            }
        }
    }

    #[instrument(
        name = "pull_gossip"
        target = LOG_TARGET,
        skip_all,
    )]
    pub(crate) async fn run_pull_gossip(self: Arc<Self>) {
        loop {
            // Some minimal timeout, to prevent this loop from accidentally going crazy
            tokio::time::sleep(Duration::from_secs(1)).await;

            let Some((peer_pubkey_to_refresh, peer_pubkey_to_query)) =
                self.pick_pull_need_gossip_pair().await
            else {
                self.peer_addr_needed().notified().await;
                continue;
            };

            if Some(peer_pubkey_to_query) == self.peer_pubkey {
                // we can't be querying our own self
                continue;
            }
            debug!(
                target: LOG_TARGET,
                %peer_pubkey_to_refresh,
                %peer_pubkey_to_query,
                "Asking for peer address"
            );

            if let Err(err) = self
                .query_peer_address(peer_pubkey_to_refresh, peer_pubkey_to_query)
                .await
            {
                debug!(
                    target: LOG_TARGET,
                    %peer_pubkey_to_refresh,
                    %peer_pubkey_to_query,
                    err = %err.fmt_compact(),
                    "Failed to query peer address"
                );
            }
        }
    }

    pub(crate) async fn query_peer_address(
        &self,
        peer_pubkey_to_check: PeerPubkey,
        peer_pubkey_to_ask: PeerPubkey,
    ) -> WhateverResult<()> {
        let mut conn = self
            .connection_pool()
            .connect(peer_pubkey_to_ask)
            .await
            .whatever_context("Failed to connect")?;

        match rpc::get_peer_address(&mut conn, peer_pubkey_to_check).await? {
            Some(update) => Self::handle_address_update(self.db(), update).await?,
            None => {
                warn!(target: LOG_TARGET,
                 %peer_pubkey_to_check,
                 %peer_pubkey_to_ask,
                 "Missing other peer address");
            }
        }

        Ok(())
    }

    async fn send_push_gossip(
        &self,
        dst_peer: PeerPubkey,
        update: Signed<AddressUpdate>,
    ) -> WhateverResult<()> {
        if Some(dst_peer) == self.peer_pubkey {
            // Just skip connecting to self
            return Ok(());
        }
        let mut conn = self
            .connection_pool()
            .connect(dst_peer)
            .await
            .whatever_context("Failed to connect")?;

        conn.make_rpc_raw(RPC_ID_PUSH_PEER_ADDR_UPDATE, |mut w, _| async move {
            w.write_message_bincode(&update).await?;
            Ok(())
        })
        .await
        .whatever_context("RPC failed")
    }

    // async fn pick_pull_gossip_pair(&self) -> Option<(PeerPubkey, PeerPubkey)> {
    //     self.db
    //         .read_with_expect(|ctx| {
    //             let tbl_tracking =
    // ctx.open_table(&peer_addresses_we_track::TABLE)?;

    //             let Some(peer_pubkey_to_check) = get_random(
    //                 &tbl_tracking,
    //                 rand::thread_rng().r#gen(),
    //                 rand::thread_rng().r#gen(),
    //             )?
    //             .map(|(k, _v)| k) else {
    //                 return Ok(None);
    //             };

    //             let tbl_updates = ctx.open_table(&peer_addresses::TABLE)?;
    //             let Some(peer_pubkey_to_ask) = get_random(
    //                 &tbl_updates,
    //                 rand::thread_rng().r#gen(),
    //                 rand::thread_rng().r#gen(),
    //             )?
    //             .map(|(k, _v)| k) else {
    //                 return Ok(None);
    //             };

    //             Ok(Some((peer_pubkey_to_check, peer_pubkey_to_ask)))
    //         })
    //         .await
    // }

    async fn pick_pull_need_gossip_pair(&self) -> Option<(PeerPubkey, PeerPubkey)> {
        self.db()
            .write_with_expect(|ctx| {
                let mut tbl_needed = ctx.open_table(&peer_addresses_we_need::TABLE)?;

                let Some(peer_pubkey_to_refresh) = get_random(
                    &tbl_needed,
                    rand::thread_rng().r#gen(),
                    rand::thread_rng().r#gen(),
                )?
                .map(|(k, _v)| k) else {
                    return Ok(None);
                };

                tbl_needed.remove(&peer_pubkey_to_refresh)?;

                let tbl_updates = ctx.open_table(&peer_addresses::TABLE)?;
                let Some(peer_pubkey_to_query) = get_random(
                    &tbl_updates,
                    rand::thread_rng().r#gen(),
                    rand::thread_rng().r#gen(),
                )?
                .map(|(k, _v)| k) else {
                    return Ok(None);
                };

                Ok(Some((peer_pubkey_to_refresh, peer_pubkey_to_query)))
            })
            .await
    }

    async fn pick_push_gossip_pair(&self) -> Option<(PeerPubkey, Signed<AddressUpdate>)> {
        self.db()
            .read_with_expect(|ctx| {
                let tbl_updates = ctx.open_table(&peer_addresses::TABLE)?;

                let Some(peer_pubkey_1) = get_random(
                    &tbl_updates,
                    rand::thread_rng().r#gen(),
                    rand::thread_rng().r#gen(),
                )?
                .map(|(k, _v)| k) else {
                    return Ok(None);
                };

                let Some(peer_pubkey_2) = get_random(
                    &tbl_updates,
                    rand::thread_rng().r#gen(),
                    rand::thread_rng().r#gen(),
                )?
                .map(|(_, v)| v) else {
                    return Ok(None);
                };

                Ok(Some((peer_pubkey_1, peer_pubkey_2)))
            })
            .await
    }

    pub(crate) async fn insert_own_address_update(&self) -> WhateverResult<()> {
        if let Some(update) = self.get_own_address_update().await? {
            Self::handle_address_update(self.db(), update).await?;
        }
        Ok(())
    }

    async fn get_own_address_update(&self) -> WhateverResult<Option<Signed<AddressUpdate>>> {
        if self.root_secret().is_none() {
            return Ok(None);
        }
        let seckey = self.get_peer_secret_expect();
        let update = Signed::new_sign(
            AddressUpdate {
                timestamp: Timestamp::now(),
                peer_pubkey: seckey.pubkey(),
                addr: PeerAddress::Iroh(self.iroh_endpoint().node_id().into()),
            },
            seckey,
        );
        Ok(Some(update))
    }

    pub async fn handle_address_update(
        db: &Database,
        update: Signed<AddressUpdate>,
    ) -> WhateverResult<()> {
        let peer_pubkey = update.peer_pubkey;
        update
            .verify_signature(peer_pubkey, update.sig)
            .whatever_context("Invalid signature")?;

        db.write_with_expect(|ctx| {
            let mut tbl = ctx.open_table(&peer_addresses::TABLE)?;

            if let Some(existing) = tbl.get(&peer_pubkey)? {
                if update.timestamp <= existing.value().timestamp {
                    return Ok(());
                }
            }
            debug!(
                target: LOG_TARGET,
                %peer_pubkey,
                "Updated peer address"
            );
            tbl.insert(&peer_pubkey, &update)?;

            Ok(())
        })
        .await;

        Ok(())
    }

    pub(crate) async fn get_peer_iroh_addr(
        db: &Database,
        peer_pubkey: PeerPubkey,
    ) -> WhateverResult<Option<iroh::NodeId>> {
        let Some(addr) = Self::get_peer_addr(db, peer_pubkey).await? else {
            return Ok(None);
        };

        match addr.addr {
            PeerAddress::Iroh(addr) => Ok(Some(addr.try_into()?)),
        }
    }

    pub(crate) async fn get_peer_addr(
        db: &Database,
        peer_pubkey: PeerPubkey,
    ) -> WhateverResult<Option<Signed<AddressUpdate>>> {
        Ok(db
            .write_with_expect(|ctx| {
                let tbl_updates = ctx.open_table(&peer_addresses::TABLE)?;

                if let Some(existing) = tbl_updates.get(&peer_pubkey)?.map(|g| g.value()) {
                    return Ok(Some(existing));
                }

                let mut tbl_tracking = ctx.open_table(&peer_addresses_we_track::TABLE)?;

                if (tbl_updates.get(&peer_pubkey)?).is_none() {
                    tbl_tracking.insert(&peer_pubkey, &())?;
                }

                Ok(None)
            })
            .await)
    }

    pub(crate) async fn mark_peer_addr_as_needed(
        &self,
        peer_pubkey: PeerPubkey,
    ) -> WhateverResult<()> {
        if Some(peer_pubkey) == self.peer_pubkey {
            // we can't be needing our own address
            return Ok(());
        }

        self.db()
            .write_with_expect(|ctx| {
                let mut tbl = ctx.open_table(&peer_addresses_we_need::TABLE)?;

                tbl.insert(&peer_pubkey, &())?;

                let notify = self.peer_addr_needed().clone();

                ctx.on_commit(move || {
                    notify.notify_waiters();
                });

                Ok(())
            })
            .await;
        Ok(())
    }
}
