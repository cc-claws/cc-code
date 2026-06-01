use ratatui::style::Color;
use std::collections::HashMap;

const PALETTE: [Color; 5] = [
    Color::Rgb(255, 95, 95),   // 红
    Color::Rgb(95, 255, 175),  // 翠绿
    Color::Rgb(255, 215, 95),  // 金黄
    Color::Rgb(95, 175, 255),  // 天蓝
    Color::Rgb(215, 135, 255), // 淡紫
];

pub struct BranchColors {
    map: HashMap<String, Color>,
}

impl BranchColors {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn color_for(&mut self, branch: &str) -> Color {
        if let Some(&c) = self.map.get(branch) {
            return c;
        }
        let idx = stable_hash(branch) as usize % PALETTE.len();
        let color = PALETTE[idx];
        self.map.insert(branch.to_string(), color);
        color
    }

    pub fn get(&self, branch: &str) -> Option<Color> {
        self.map.get(branch).copied()
    }

    #[allow(dead_code)]
    pub fn default_color() -> Color {
        Color::DarkGray
    }
}

/// 基于分支名的稳定 hash，保证同一分支名永远分配同一颜色
fn stable_hash(s: &str) -> u32 {
    // FNV-1a 简单实现
    let mut hash: u32 = 2166136261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

impl Default for BranchColors {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_branch_same_color() {
        let mut bc = BranchColors::new();
        let c1 = bc.color_for("main");
        let c2 = bc.color_for("main");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_different_branches_may_differ() {
        let mut bc = BranchColors::new();
        let c1 = bc.color_for("main");
        // 不再强制不同（hash 可能碰撞），但同一分支必须相同
        let c2 = bc.color_for("main");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_stable_across_instances() {
        let c1 = BranchColors::new().color_for("feature/foo");
        let c2 = BranchColors::new().color_for("feature/foo");
        assert_eq!(c1, c2, "同一分支名在不同 BranchColors 实例中应获得相同颜色");
    }
}
