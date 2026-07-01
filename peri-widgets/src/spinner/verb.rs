use rand::RngExt;

/// 中文随机动词，用于 loading spinner 显示。
pub const ZH_VERBS: &[&str] = &[
    // ── 烹饪/烘焙类 ──
    "烹制中",
    "烘焙中",
    "煎制中",
    "炖煮中",
    "慢炖中",
    "调味中",
    "腌制中",
    "加热中",
    "翻炒中",
    "焖制中",
    "蒸制中",
    // ── 思考/分析类 ──
    "思考中",
    "分析中",
    "计算中",
    "推理中",
    "推敲中",
    "斟酌中",
    "琢磨中",
    "沉思中",
    "反思中",
    "酝酿中",
    "深思中",
    "考虑中",
    // ── 创作/构建类 ──
    "编写中",
    "构建中",
    "创作中",
    "设计中",
    "勾勒中",
    "绘制中",
    "编排中",
    "编舞中",
    "雕琢中",
    "锻造中",
    "打磨中",
    "装饰中",
    // ── 搜索/处理类 ──
    "处理中",
    "搜索中",
    "检索中",
    "读取中",
    "扫描中",
    "核对中",
    "编译中",
    "合并中",
    "转换中",
    "解析中",
    // ── 动作/运动类 ──
    "执行中",
    "运行中",
    "跳跃中",
    "舞动中",
    "游荡中",
    "漫步中",
    "飞驰中",
    "追踪中",
    "漂移中",
    "盘旋中",
    "滑行中",
    "旋转中",
    "摇摆中",
    // ── 幻想/创意类 ──
    "魔法中",
    "变形中",
    "传送中",
    "炼金中",
    "召唤中",
    "充能中",
    "酿造中",
    "施法中",
    "觉醒中",
    "融合中",
    "量子中",
    // ── 自然/生长类 ──
    "生长中",
    "发芽中",
    "开花中",
    "扎根中",
    "蔓延中",
    "进化中",
    "孵化中",
    "授粉中",
    "光合中",
    "蒸腾中",
    "结霜中",
    // ── 幽默/俏皮类 ──
    "捣鼓中",
    "摆弄中",
    "折腾中",
    "玩耍中",
    "闲逛中",
    "摸鱼中",
    "发呆中",
    "神游中",
    "纳闷中",
    "挠头中",
    "迷路中",
    "打盹中",
    // ── 概念/抽象类 ──
    "重组中",
    "折叠中",
    "编织中",
    "凝结中",
    "升华中",
    "沉淀中",
    "萌芽中",
    "结晶中",
    "聚合中",
    "校准中",
    "同步中",
    // ── 其他 ──
    "工作中",
    "打造中",
    "收集中",
    "整理中",
    "探索中",
    "巡查中",
    "监控中",
    "实验中",
    "检验中",
    "测试中",
];

/// 向后兼容别名
pub const DEFAULT_VERBS: &[&str] = ZH_VERBS;

/// 英文随机动词，用于 loading spinner 显示。
pub const EN_VERBS: &[&str] = &[
    // ── Cooking/Baking ──
    "Cooking",
    "Baking",
    "Frying",
    "Simmering",
    "Stewing",
    "Seasoning",
    "Marinating",
    "Heating",
    "Stir-frying",
    "Braising",
    "Steaming",
    // ── Thinking/Analysis ──
    "Thinking",
    "Analyzing",
    "Computing",
    "Reasoning",
    "Pondering",
    "Deliberating",
    "Reflecting",
    "Meditating",
    "Contemplating",
    "Mulling",
    "Weighing",
    "Considering",
    // ── Creating/Building ──
    "Writing",
    "Building",
    "Creating",
    "Designing",
    "Sketching",
    "Painting",
    "Composing",
    "Choreographing",
    "Carving",
    "Forging",
    "Polishing",
    "Decorating",
    // ── Processing/Searching ──
    "Processing",
    "Searching",
    "Retrieving",
    "Reading",
    "Scanning",
    "Verifying",
    "Compiling",
    "Merging",
    "Converting",
    "Parsing",
    // ── Motion/Action ──
    "Executing",
    "Running",
    "Jumping",
    "Dancing",
    "Wandering",
    "Strolling",
    "Dashing",
    "Tracking",
    "Drifting",
    "Circling",
    "Gliding",
    "Spinning",
    "Swaying",
    // ── Fantasy/Creative ──
    "Conjuring",
    "Transforming",
    "Teleporting",
    "Transmuting",
    "Summoning",
    "Charging",
    "Brewing",
    "Casting",
    "Awakening",
    "Fusing",
    "Quantizing",
    // ── Nature/Growth ──
    "Growing",
    "Sprouting",
    "Blooming",
    "Rooting",
    "Spreading",
    "Evolving",
    "Hatching",
    "Pollinating",
    "Photosynthesizing",
    "Transpiring",
    "Frosting",
    // ── Playful/Quirky ──
    "Tinkering",
    "Fidgeting",
    "Fussing",
    "Playing",
    "Loitering",
    "Slacking",
    "Daydreaming",
    "Wandering off",
    "Puzzling",
    "Scratching head",
    "Getting lost",
    "Napping",
    // ── Conceptual/Abstract ──
    "Recombining",
    "Folding",
    "Weaving",
    "Condensing",
    "Sublimating",
    "Precipitating",
    "Germinating",
    "Crystallizing",
    "Aggregating",
    "Calibrating",
    "Syncing",
    // ── Other ──
    "Working",
    "Crafting",
    "Collecting",
    "Organizing",
    "Exploring",
    "Patrolling",
    "Monitoring",
    "Experimenting",
    "Inspecting",
    "Testing",
];

/// 根据语言标识选择动词列表。
pub fn verbs_for_lang(lang: &str) -> &'static [&'static str] {
    if lang.starts_with("zh") {
        ZH_VERBS
    } else {
        EN_VERBS
    }
}

pub fn pick_verb(active_form: Option<&str>) -> String {
    pick_verb_from(active_form, DEFAULT_VERBS)
}

/// 从指定动词列表随机选取，支持外部传入翻译后的动词列表。
pub fn pick_verb_from(active_form: Option<&str>, verbs: &[&str]) -> String {
    active_form.map(|s| format!("{}…", s)).unwrap_or_else(|| {
        let mut rng = rand::rng();
        verbs[rng.random_range(0..verbs.len())].to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_verb_with_active_form() {
        let result = pick_verb(Some("搜索文件"));
        assert!(
            result.contains("搜索文件…"),
            "expected '搜索文件…', got '{}'",
            result
        );
    }

    #[test]
    fn test_pick_verb_random() {
        let result = pick_verb(None);
        assert!(!result.is_empty(), "verb should not be empty");
        assert!(
            DEFAULT_VERBS.contains(&result.as_str()),
            "'{}' should be in DEFAULT_VERBS",
            result
        );
    }

    #[test]
    fn test_en_verbs_not_empty() {
        assert!(!EN_VERBS.is_empty(), "EN_VERBS should not be empty");
        assert_eq!(EN_VERBS.len(), ZH_VERBS.len(), "EN_VERBS and ZH_VERBS should have the same length");
    }

    #[test]
    fn test_verbs_for_lang() {
        assert_eq!(verbs_for_lang("en"), EN_VERBS);
        assert_eq!(verbs_for_lang("en-US"), EN_VERBS);
        assert_eq!(verbs_for_lang("zh-CN"), ZH_VERBS);
        assert_eq!(verbs_for_lang("zh"), ZH_VERBS);
        // 未知语言回退英文
        assert_eq!(verbs_for_lang("fr"), EN_VERBS);
    }
}
