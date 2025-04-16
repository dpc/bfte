use std::ops;

use bincode::{Decode, Encode};
use redb_bincode::{ReadableTable, StorageError};

pub fn get_random<K, V>(
    tbl: &impl ReadableTable<K, V>,
    random_pivot: K,
    coin_flip: bool,
) -> Result<Option<(K, V)>, StorageError>
where
    K: Decode<()> + Encode + Clone,
    V: Decode<()> + Encode,
{
    let before_pivot = ..(random_pivot.clone());
    let after_pivot = random_pivot..;

    Ok(Some(if coin_flip {
        match get_first_in_range(tbl, after_pivot)? {
            Some(k) => k,
            _ => match get_last_in_range(tbl, before_pivot)? {
                Some(k) => k,
                _ => {
                    return Ok(None);
                }
            },
        }
    } else {
        match get_first_in_range(tbl, before_pivot)? {
            Some(k) => k,
            _ => match get_last_in_range(tbl, after_pivot)? {
                Some(k) => k,
                _ => {
                    return Ok(None);
                }
            },
        }
    }))
}

fn get_first_in_range<K, V>(
    tbl: &impl ReadableTable<K, V>,
    range: impl ops::RangeBounds<K>,
) -> Result<Option<(K, V)>, StorageError>
where
    K: bincode::Decode<()> + bincode::Encode,
    V: bincode::Decode<()> + bincode::Encode,
{
    Ok(tbl
        .range(range)?
        .next()
        .transpose()?
        .map(|(k, v)| (k.value(), v.value())))
}

fn get_last_in_range<K, V>(
    tbl: &impl ReadableTable<K, V>,
    range: impl ops::RangeBounds<K>,
) -> Result<Option<(K, V)>, StorageError>
where
    K: bincode::Decode<()> + bincode::Encode,
    V: bincode::Decode<()> + bincode::Encode,
{
    Ok(tbl
        .range(range)?
        .next_back()
        .transpose()?
        .map(|(k, v)| (k.value(), v.value())))
}
