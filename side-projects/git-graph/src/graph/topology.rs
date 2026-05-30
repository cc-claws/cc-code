use crate::git::commit::TopoNode;
use crate::git::stash::StashInfo;
use git2::Oid;
use std::collections::HashMap;

/// 拓扑骨架
#[allow(dead_code)]
pub struct Topology {
    nodes: Vec<TopoNode>,
    index: HashMap<Oid, usize>,
    branch_map: HashMap<Oid, Vec<String>>,
    tag_map: HashMap<Oid, Vec<String>>,
    stash_map: HashMap<Oid, Vec<StashInfo>>,
}

#[allow(dead_code)]
impl Topology {
    pub fn new(
        mut nodes: Vec<TopoNode>,
        branch_map: HashMap<Oid, Vec<String>>,
        tag_map: HashMap<Oid, Vec<String>>,
        stash_map: HashMap<Oid, Vec<StashInfo>>,
    ) -> Self {
        // 按时间降序排列（最新在前）
        nodes.sort_by_key(|b| std::cmp::Reverse(b.time));
        let index: HashMap<Oid, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.oid, i)).collect();
        Self {
            nodes,
            index,
            branch_map,
            tag_map,
            stash_map,
        }
    }

    pub fn nodes(&self) -> &[TopoNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&TopoNode> {
        self.nodes.get(idx)
    }

    pub fn index_of(&self, oid: Oid) -> Option<usize> {
        self.index.get(&oid).copied()
    }

    pub fn branches_for(&self, oid: Oid) -> &[String] {
        self.branch_map
            .get(&oid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn tags_for(&self, oid: Oid) -> &[String] {
        self.tag_map.get(&oid).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn stashes_for(&self, oid: Oid) -> &[StashInfo] {
        self.stash_map
            .get(&oid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn branch_map_raw(&self) -> HashMap<Oid, Vec<String>> {
        self.branch_map.clone()
    }

    pub fn stash_map(&self) -> &HashMap<Oid, Vec<StashInfo>> {
        &self.stash_map
    }

    pub fn tag_map(&self) -> &HashMap<Oid, Vec<String>> {
        &self.tag_map
    }
}
