use std::io;

use redb_bincode::redb;
use snafu::{Location, Snafu};
use tokio::task::JoinError;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum DbError {
    #[snafu(display("Database error at {location}"))]
    Database {
        source: redb::DatabaseError,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(transparent)]
    Table {
        source: redb::TableError,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(transparent)]
    Storage {
        source: redb::StorageError,
        #[snafu(implicit)]
        location: Location,
    },
    Transaction {
        source: redb::TransactionError,
        #[snafu(implicit)]
        location: Location,
    },
    Join {
        source: JoinError,
        #[snafu(implicit)]
        location: Location,
    },
    Commit {
        source: redb::CommitError,
        #[snafu(implicit)]
        location: Location,
    },
    DbVersionTooHigh {
        db_ver: u64,
        code_ver: u64,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(transparent)]
    Io {
        source: io::Error,
        #[snafu(implicit)]
        location: Location,
    },
    InvalidPath {
        #[snafu(implicit)]
        location: Location,
    },
    Overflow,
}

pub type DbResult<T> = std::result::Result<T, DbError>;

impl<E> TryFrom<DbTxError<E>> for DbError
where
    E: snafu::Error,
{
    type Error = E;

    fn try_from(value: DbTxError<E>) -> Result<Self, Self::Error> {
        match value {
            DbTxError::DbError {
                source,
                location: _,
            } => Ok(source),
            DbTxError::TxError {
                source,
                location: _,
            } => Err(source),
        }
    }
}

/// Database transaction error with a user-defined application error
///
/// Basically, it can either be some kind of a database issue bubbling
/// up, or whatever error `E` the user needs for the db transaction logic
/// to error out.
///
/// This type might be a bit hard to use. Maybe I'm overcomplicating
/// it. --dpc
///
/// Look up existing usages, and be aware of helper functions like
/// [`DbTxError::tx_into`], [`DbTxError::map`], and custom `From`
/// implementations to help with it.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum DbTxError<E>
where
    E: snafu::Error + 'static,
{
    #[snafu(transparent)]
    DbError {
        source: DbError,

        #[snafu(implicit)]
        location: Location,
    },
    TxError {
        source: E,
        #[snafu(implicit)]
        location: Location,
    },
}

impl<E> From<redb::TableError> for DbTxError<E>
where
    E: snafu::Error,
{
    fn from(value: redb::TableError) -> Self {
        DbError::from(value).into()
    }
}

impl<E> From<redb::StorageError> for DbTxError<E>
where
    E: snafu::Error,
{
    fn from(value: redb::StorageError) -> Self {
        DbError::from(value).into()
    }
}

/// A `Result` with `T` as success, and [`DbTxError`] as the error.
pub type DbTxResult<T, E> = std::result::Result<T, DbTxError<E>>;

impl<E> DbTxError<E>
where
    E: snafu::Error,
{
    pub fn tx_into<E2>(self) -> DbTxError<E2>
    where
        E2: From<E> + snafu::Error,
    {
        match self {
            DbTxError::DbError { source, location } => DbTxError::DbError { source, location },
            DbTxError::TxError { source, location } => DbTxError::TxError {
                source: source.into(),
                location,
            },
        }
    }
}
impl<E> DbTxError<E>
where
    E: snafu::Error,
{
    pub fn map<E2>(self, f: impl FnOnce(E) -> E2) -> DbTxError<E2>
    where
        E2: snafu::Error,
    {
        match self {
            DbTxError::DbError { source, location } => DbTxError::DbError { source, location },
            DbTxError::TxError { source, location } => DbTxError::TxError {
                source: f(source),
                location,
            },
        }
    }
}
