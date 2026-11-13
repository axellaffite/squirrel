use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Folder,
    File,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown node kind: {0}")]
pub struct UnknownNodeKind(pub String);

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Folder => "folder",
            NodeKind::File => "file",
        }
    }
}

impl FromStr for NodeKind {
    type Err = UnknownNodeKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "folder" => Ok(NodeKind::Folder),
            "file" => Ok(NodeKind::File),
            other => Err(UnknownNodeKind(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for kind in [NodeKind::Folder, NodeKind::File] {
            assert_eq!(kind.as_str().parse::<NodeKind>().unwrap(), kind);
        }
        assert!("symlink".parse::<NodeKind>().is_err());
    }
}
