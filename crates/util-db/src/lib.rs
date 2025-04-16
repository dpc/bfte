// SPDX-License-Identifier: MIT

pub mod random;

pub use ::redb_bincode;

#[macro_export]
macro_rules! def_table {
    ($(#[$outer:meta])*
        $name:ident : $k:ty => $v:ty) => {
        #[allow(unused)]
        $(#[$outer])*
        pub mod $name {
            use super::*;
            pub type Key = $k;
            pub type Value = $v;
            pub type Definition<'a> = $crate::redb_bincode::TableDefinition<'a, Key, Value>;
            pub trait ReadableTable: $crate::redb_bincode::ReadableTable<Key, Value> {}
            impl<RT> ReadableTable for RT where RT: $crate::redb_bincode::ReadableTable<Key, Value> {}
            pub type Table<'a> = $crate::redb_bincode::Table<'a, Key, Value>;
            pub const TABLE: Definition = $crate::redb_bincode::TableDefinition::new(stringify!($name));
        }
    };
}
