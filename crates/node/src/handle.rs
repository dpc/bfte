use std::marker::PhantomData;
use std::ops;
use std::sync::{Arc, Weak};

use bfte_db::Database;
use bfte_util_error::WhateverResult;
use snafu::{Location, OptionExt as _, ResultExt as _, Snafu};

use crate::Node;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub struct NodeRefError {
    #[snafu(implicit)]
    location: Location,
}
pub type NodeRefResult<T> = Result<T, NodeRefError>;

pub trait NodeRefResultExt<T> {
    fn into_whatever(self) -> WhateverResult<T>;
}

impl<T> NodeRefResultExt<T> for NodeRefResult<T> {
    fn into_whatever(self) -> WhateverResult<T> {
        self.whatever_context("Client gone")
    }
}

/// A strong reference to [`Node`]
///
/// It contains a phantom reference, to avoid attempts of
/// storing it anywhere.
#[derive(Clone)]
pub struct NodeRef<'r> {
    pub(crate) node: Arc<Node>,
    pub(crate) r: PhantomData<&'r ()>,
}

impl ops::Deref for NodeRef<'_> {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

/// Weak handle to [`Node`]
#[derive(Debug, Clone)]
pub struct NodeHandle(Weak<Node>);

impl From<Weak<Node>> for NodeHandle {
    fn from(value: Weak<Node>) -> Self {
        Self(value)
    }
}

impl NodeHandle {
    pub fn node_ref(&self) -> NodeRefResult<NodeRef<'_>> {
        let client = self.0.upgrade().context(NodeRefSnafu)?;
        Ok(NodeRef {
            node: client,
            r: PhantomData,
        })
    }

    #[allow(dead_code)]
    pub fn db(&self) -> NodeRefResult<Arc<Database>> {
        let client = self.0.upgrade().context(NodeRefSnafu)?;

        Ok(client.db().clone())
    }
}
