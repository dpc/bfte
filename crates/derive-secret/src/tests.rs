use hex_literal::hex;

use super::*;

#[test]
fn derive_secret_sanity() {
    let root = DeriveableSecret {
        bytes: [1; 32],
        level: 0,
    };

    assert_eq!(
        root.reveal_bytes(),
        hex!("0101010101010101010101010101010101010101010101010101010101010101")
    );
    root.ensure_level(0).expect("Not fail");
    root.ensure_level(1).expect_err("Must fail");
    root.derive(ChildId(0)).ensure_level(1).expect("Must fail");

    assert_eq!(
        root.derive(ChildId(0)).reveal_bytes(),
        hex!("d037d677a9d639578ca0b7edf44f3b0eb91b429e6e1660ca3fb2cabca566290d")
    );
    assert_eq!(
        root.derive(ChildId(1)).reveal_bytes(),
        hex!("e0192cf796a4bb729101cc60d18e479fc8727ffbb2144b2418ac781aa95c3a88")
    );
    assert_eq!(
        root.derive(ChildId(1)).derive(ChildId(0)).reveal_bytes(),
        hex!("086e97d3f10745bf0d75a46deb693f7332b3b8f1eb41bd7ab4dda2add1f28e19")
    );
}
