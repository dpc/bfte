use bfte_consensus_core::peer::PeerSeckey;
use bfte_derive_secret::{ChildId, LevelResult};

const PEER_SECKEY_CHILD_ID: ChildId = ChildId::new(0);
const IROH_SECRET_CHILD_ID: ChildId = ChildId::new(1);

pub trait DeriveSecretExt {
    fn get_peer_seckey(self) -> LevelResult<PeerSeckey>;
    fn get_iroh_secret(self) -> LevelResult<iroh::SecretKey>;
}

impl DeriveSecretExt for bfte_derive_secret::DeriveableSecret {
    fn get_peer_seckey(self) -> LevelResult<PeerSeckey> {
        self.ensure_level(0)?;
        Ok(self.derive(PEER_SECKEY_CHILD_ID).reveal_bytes().into())
    }

    fn get_iroh_secret(self) -> LevelResult<iroh::SecretKey> {
        self.ensure_level(0)?;
        Ok(self.derive(IROH_SECRET_CHILD_ID).reveal_bytes().into())
    }
}
