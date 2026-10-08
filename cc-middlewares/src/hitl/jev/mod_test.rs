//! mod.rs 单元测试（确定性层，不打网络）

use super::*;
use std::path::PathBuf;
use std::sync::Arc;

const CWD: &str = "/home/u/project";

fn gate() -> Arc<JevGate> {
    // 无 key 也 OK —— 确定性层不依赖 key
    JevGate::new(JevConfig {
        api_key_env: "JEV_UNSET_FOR_TEST".to_string(),
        ..Default::default()
    })
    .unwrap()
}

fn bash_call(cmd: &str, cwd: &std::path::Path) -> GateCall {
    GateCall {
        tool_name: "Bash".to_string(),
        command: Some(cmd.to_string()),
        original_command: None,
        path: None,
        branch: None,
        cwd: cwd.to_path_buf(),
    }
}

#[test]
fn test_hard_deny_blocks_without_jev() {
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = bash_call("rm -rf /", &cwd);
    // 硬黑名单 → Block（即使无 key 也不影响）
    let d = g.deterministic(&call);
    assert!(matches!(d, Some(GateDecision::Block { .. })));
}

#[test]
fn test_read_only_allows_without_jev() {
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = bash_call("git status", &cwd);
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_user_allow_rule() {
    let cwd = PathBuf::from(CWD);
    let g = JevGate::new(JevConfig {
        api_key_env: "JEV_UNSET_FOR_TEST".to_string(),
        allowed_commands: vec!["cargo build*".to_string()],
        ..Default::default()
    })
    .unwrap();
    let call = bash_call("cargo build", &cwd);
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_user_deny_rule() {
    let cwd = PathBuf::from(CWD);
    let g = JevGate::new(JevConfig {
        api_key_env: "JEV_UNSET_FOR_TEST".to_string(),
        disallowed_commands: vec!["kubectl delete*".to_string()],
        ..Default::default()
    })
    .unwrap();
    let call = bash_call("kubectl delete pod x", &cwd);
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Block { .. })
    ));
}

#[test]
fn test_dangerous_escalates_to_jev() {
    // `sudo rm -rf /var/...` 含 sudo 危险形状 → 确定性层不判决（返回 None，走 Jev）
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = bash_call("sudo apt install x", &cwd);
    assert!(g.deterministic(&call).is_none());
}

#[test]
fn test_protected_write_escalates() {
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let path = PathBuf::from(CWD).join(".env");
    let call = GateCall {
        tool_name: "Write".to_string(),
        command: None,
        original_command: None,
        path: Some(path.clone()),
        branch: None,
        cwd: cwd.clone(),
    };
    // 受保护路径 → 不直接放行（走 Jev）
    assert!(g.deterministic(&call).is_none());
}

#[test]
fn test_safe_write_allows() {
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let path = PathBuf::from(CWD).join("src/main.rs");
    let call = GateCall {
        tool_name: "Write".to_string(),
        command: None,
        original_command: None,
        path: Some(path.clone()),
        branch: None,
        cwd: cwd.clone(),
    };
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_judge_unavailable_asks_instead_of_blocking() {
    // 无 key → 语义层判定不可用。**不可用 ≠ 违规**。
    // 官方语义：network failure / timeout / error / unreadable body 一律视为 unknown，
    // 回到正常的人工确认——多弹一次窗，而不是执行危险命令，也不是把 agent 堵死。
    // （实测教训：端点 503 时 fail-closed 会把整个会话的工具调用全堵住。）
    //
    // 注意：这里用 `git reset --hard`（命中 DANGEROUS、升级 Jev），
    // **不能**用 `curl|bash`——它已在硬黑名单，会在到达语义层之前就被拦掉。
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = bash_call("git reset --hard HEAD~1", &cwd);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let d = rt.block_on(g.evaluate(&call));
    assert!(
        matches!(d, GateDecision::Ask { .. }),
        "判定不可用应回到人工确认，expected Ask, got {d:?}"
    );
}

#[test]
fn test_find_exec_not_read_only() {
    // C1 回归：find 带 -exec 必须不进入只读快车道
    let g = gate();
    let cwd = PathBuf::from(CWD);
    for cmd in [
        "find . -exec rm {} +",
        "find . -execdir rm {} +",
        "find . -delete",
        "find . -exec sh -c whoami +",
    ] {
        assert!(
            g.deterministic(&bash_call(cmd, &cwd)).is_none(),
            "{cmd} 不应走只读快车道"
        );
    }
}

#[test]
fn test_template_credential_not_bypassed() {
    // C2 回归：混入 .env.example 不能豁免真实 .env 的读取
    let g = gate();
    let cwd = PathBuf::from(CWD);
    for cmd in ["cat .env.example .env", "cat .env .env.example"] {
        assert!(
            g.deterministic(&bash_call(cmd, &cwd)).is_none(),
            "{cmd} 应升级到 Jev（读到真实 .env）"
        );
    }
    // 纯模板读取仍可走快车道
    assert!(matches!(
        g.deterministic(&bash_call("cat .env.example", &cwd)),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_execute_extra_tool_params_unwrapped() {
    // C3 回归：ExecuteExtraTool 的 command 在 params 里，必须被解包
    let input = serde_json::json!({
        "tool_name": "Bash",
        "params": { "command": "rm -rf /" }
    });
    let params = effective_params("ExecuteExtraTool", &input);
    assert_eq!(
        params.get("command").and_then(|v| v.as_str()),
        Some("rm -rf /")
    );
    // 普通工具不解包
    let plain = serde_json::json!({ "command": "ls" });
    assert_eq!(
        effective_params("Bash", &plain)
            .get("command")
            .and_then(|v| v.as_str()),
        Some("ls")
    );
    // 解包后的命令确实会被硬黑名单拦截
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let cmd = params.get("command").and_then(|v| v.as_str()).unwrap();
    assert!(matches!(
        g.deterministic(&bash_call(cmd, &cwd)),
        Some(GateDecision::Block { .. })
    ));
}

#[test]
fn test_compose_policy_labels_and_order() {
    // Arrange: 按权威性降序传入（key, label, content）
    let out = compose_policy(
        &[
            ("personal", "Priority 1 — personal", Some("不要动生产库")),
            (
                "project",
                "Priority 2 — project",
                Some("禁止 git push --force"),
            ),
            ("global", "Priority 3 — global", Some("总是用中文回答")),
        ],
        10_000,
    );
    // Assert: 三段都在，且保持传入顺序
    let personal = out.find("## Priority 1").unwrap();
    let project = out.find("## Priority 2").unwrap();
    let global = out.find("## Priority 3").unwrap();
    assert!(personal < project && project < global, "policy: {out}");
    assert!(out.contains("不要动生产库"));
    assert!(out.contains("总是用中文回答"));
}

#[test]
fn test_compose_policy_marks_source_key() {
    // source= 是模型给每条规则标注来源的依据，必须出现在每个区块标题里
    let out = compose_policy(
        &[
            ("personal", "P1", Some("个人规则")),
            ("global", "P3", Some("全局规则")),
        ],
        10_000,
    );
    assert!(out.contains("| source=personal"), "policy: {out}");
    assert!(out.contains("| source=global"), "policy: {out}");
}

#[test]
fn test_source_label_maps_known_keys() {
    // 每个 key 都要能翻成人话，且不能翻成它自己（否则等于没翻）
    for (key, label) in SOURCE_LABELS {
        assert_ne!(source_label(key), *key, "缺少 {key} 的中文名");
        assert!(!label.is_empty());
    }
    assert!(source_label("project").contains("项目"));
    // 未知 key 原样返回，不至于丢掉信息
    assert_eq!(source_label("whatever"), "whatever");
}

#[test]
fn test_compose_policy_skips_missing_and_blank() {
    let out = compose_policy(
        &[
            ("a", "A", None),
            ("b", "B", Some("   \n ")),
            ("c", "C", Some("真规则")),
        ],
        10_000,
    );
    assert_eq!(out, "## C | source=c\n真规则");
}

#[test]
fn test_compose_policy_truncates_tail() {
    // 超长时从尾部截断：头部完整保留，尾部被砍（可能只剩前缀）
    let long_a = "甲".repeat(50);
    let long_b = "乙".repeat(5000);
    let out = compose_policy(
        &[
            ("head", "Head", Some(&long_a)),
            ("tail", "Tail", Some(&long_b)),
        ],
        200,
    );
    assert!(out.starts_with("## Head"), "out: {out}");
    assert!(out.contains(&"甲".repeat(50)), "头部应完整: {out}");
    assert!(out.chars().count() <= 203, "总长应受 200 限制: {out}");
    let yi = out.matches('乙').count();
    assert!(yi < 5000, "尾部应被截断，实际 {} 个乙", yi);
}

// ─── 显式配置的优先级梯 ─────────────────────────────────────────────────────

fn gate_with(config: JevConfig) -> Arc<JevGate> {
    JevGate::new(JevConfig {
        api_key_env: "JEV_UNSET_FOR_TEST".to_string(),
        ..config
    })
    .unwrap()
}

#[test]
fn test_deny_rule_matches_original_command_after_rtk_rewrite() {
    // #358：Bash 会先经 RTK 前缀改写（X → `rtk X`，见 gate_effective_call）。
    // 用户写的是 `kubectl delete*`，若只拿改写后的 `rtk kubectl delete pod x` 去匹配，
    // 规则会**静默失效**——装了 rtk 的机器上审批门形同虚设。
    let g = gate_with(JevConfig {
        disallowed_commands: vec!["kubectl delete*".to_string()],
        ..JevConfig::default()
    });
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Bash".to_string(),
        command: Some("rtk kubectl delete pod x".to_string()),
        original_command: Some("kubectl delete pod x".to_string()),
        path: None,
        branch: None,
        cwd,
    };
    assert!(
        matches!(g.deterministic(&call), Some(GateDecision::Block { .. })),
        "原始命令命中拒绝规则必须拦下"
    );
}

#[test]
fn test_allow_rule_matches_original_command_after_rtk_rewrite() {
    // 反向对称：用户白名单同样不能被 `rtk ` 前缀弄失效
    let g = gate_with(JevConfig {
        allowed_commands: vec!["git push origin*".to_string()],
        ..JevConfig::default()
    });
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Bash".to_string(),
        command: Some("rtk git push origin main".to_string()),
        original_command: Some("git push origin main".to_string()),
        path: None,
        branch: None,
        cwd,
    };
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_deny_rule_on_rewritten_command_also_blocks() {
    // 另一侧也要成立：规则若只命中**改写后**的命令，同样拦
    let g = gate_with(JevConfig {
        disallowed_commands: vec!["rtk kubectl get*".to_string()],
        ..JevConfig::default()
    });
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Bash".to_string(),
        command: Some("rtk kubectl get pods".to_string()),
        original_command: Some("kubectl get pods".to_string()),
        path: None,
        branch: None,
        cwd,
    };
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Block { .. })
    ));
}

#[test]
fn test_read_only_fast_lane_survives_transparent_rtk_wrap() {
    // rtk 是**输出过滤型透明包装**：`rtk git status` 与 `git status` 的只读性质一致，
    // 必须仍走零成本快车道——否则装了 rtk 的机器上每条只读命令都要多走一次判定/分类调用。
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Bash".to_string(),
        command: Some("rtk git status".to_string()),
        original_command: Some("git status".to_string()),
        path: None,
        branch: None,
        cwd,
    };
    assert!(
        matches!(g.deterministic(&call), Some(GateDecision::Allow { .. })),
        "透明包装不得让只读快车道失效"
    );
}

#[test]
fn test_read_only_fast_lane_rejected_for_opaque_rewrite() {
    // 反向：包装**不透明**（有效命令 ≠ `rtk <原始>`）时，按有效命令判定 → 不进快车道
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Bash".to_string(),
        command: Some("rtk git status".to_string()),
        original_command: Some("rm -rf /".to_string()),
        path: None,
        branch: None,
        cwd,
    };
    // 不变量是「**不得**走快车道放行」；此处原始命令命中硬黑名单，故结果是 Block 而非 None
    assert!(
        !matches!(g.deterministic(&call), Some(GateDecision::Allow { .. })),
        "不透明改写不得进快车道"
    );
}

#[test]
fn test_explicit_deny_still_beats_explicit_allow() {
    // 同一信任级内保持 deny 优先
    let g = gate_with(JevConfig {
        allowed_commands: vec!["git push*".to_string()],
        disallowed_commands: vec!["git push origin prod*".to_string()],
        ..Default::default()
    });
    let cwd = PathBuf::from(CWD);
    assert!(matches!(
        g.deterministic(&bash_call("git push origin prod", &cwd)),
        Some(GateDecision::Block { .. })
    ));
}

#[test]
fn test_hard_deny_unaffected_by_explicit_allow() {
    // 硬黑名单不可被任何显式规则放行
    let g = gate_with(JevConfig {
        allowed_commands: vec!["rm -rf /*".to_string()],
        ..Default::default()
    });
    let cwd = PathBuf::from(CWD);
    assert!(matches!(
        g.deterministic(&bash_call("rm -rf /", &cwd)),
        Some(GateDecision::Block { .. })
    ));
}

// ─── 拦截说明（面向人和 agent）────────────────────────────────────────────────

#[test]
fn test_block_message_shows_rule_text_not_internal_id() {
    // 用户要知道"是哪条规则拦的"，而规则必须是他自己写的原文
    let g = gate_with(JevConfig {
        policy: "1. 禁止泄露真实的 Windows 硬件地址（MAC）。\n2. 不得查询、输出或发送该信息。"
            .to_string(),
        ..Default::default()
    });
    let (user, _agent) = g.block_messages("policy_compliance");
    assert!(
        user.contains("禁止泄露真实的 Windows 硬件地址"),
        "user: {user}"
    );
    assert!(user.contains("不得查询"), "user: {user}");
    assert!(
        !user.contains("policy_compliance"),
        "用户文案不应暴露内部规则 id: {user}"
    );
}

#[test]
fn test_user_message_has_no_internal_ids_but_agent_message_does() {
    // 两类读者需求不同：用户要**易懂**（不能有内部术语），agent 要**专业**（必须有 rule id）
    let g = gate_with(JevConfig {
        policy: "1. 禁止泄露真实的 Windows 硬件地址（MAC）。".to_string(),
        ..Default::default()
    });
    let (user, agent) = g.block_messages("policy_compliance");

    assert!(
        user.contains("禁止泄露真实的 Windows 硬件地址"),
        "user: {user}"
    );
    assert!(
        !user.contains("policy_compliance"),
        "用户文案不应出现内部规则 id: {user}"
    );

    assert!(agent.contains("policy_compliance"), "agent: {agent}");
    assert!(agent.contains("retryable=no"), "agent: {agent}");
    assert!(agent.contains("Do not retry"), "agent: {agent}");
    assert!(
        agent.contains("execute it manually in their terminal"),
        "agent: {agent}"
    );
}

#[test]
fn test_non_policy_rule_messages() {
    let g = gate();
    let (user, agent) = g.block_messages("no_secret_egress");
    assert!(user.contains("密钥"), "user: {user}");
    assert!(
        !user.contains("no_secret_egress"),
        "用户文案不应出现内部规则 id: {user}"
    );
    assert!(agent.contains("no_secret_egress"), "agent: {agent}");
    assert!(agent.contains("内置安全条件"), "agent: {agent}");
}

#[test]
fn test_ask_message_also_shows_rule_text() {
    let g = gate_with(JevConfig {
        policy: "禁止改动 CI 配置。".to_string(),
        ..Default::default()
    });
    let msg = g.ask_message("policy_compliance");
    assert!(msg.contains("禁止改动 CI 配置"), "msg: {msg}");
}

#[test]
fn test_block_message_groups_rules_by_source_file() {
    // 用户要知道的不只是"违反了规则"，而是"哪条规则、在哪个文件里"
    let g = gate();
    *g.rules_slot.write() = Some(Arc::new(JevRules {
        rules: vec![
            JevRule {
                text: "禁止泄露真实的 Windows 硬件地址（MAC）".to_string(),
                source: "project".to_string(),
            },
            JevRule {
                text: "不要动生产库".to_string(),
                source: "personal".to_string(),
            },
        ],
        protected_paths: vec![],
    }));

    let (user, _agent) = g.block_messages("policy_compliance");
    assert!(user.contains("项目规则 CLAUDE.md"), "user: {user}");
    assert!(user.contains("个人规则 CLAUDE.local.md"), "user: {user}");
    assert!(
        user.contains("禁止泄露真实的 Windows 硬件地址"),
        "user: {user}"
    );
    assert!(user.contains("不要动生产库"), "user: {user}");
    assert!(
        !user.contains("policy_compliance"),
        "用户文案不应暴露内部规则 id: {user}"
    );
}

#[test]
fn test_write_path_traversal_must_not_fast_lane() {
    // P0 审计：`path.starts_with(cwd)` 是**词法**前缀比较。
    // `cwd/../../etc/passwd` 词法上确实以 cwd 开头 → 会被当成"项目内非受保护路径"
    // 直接放行，从而**完全跳过 Jev 判定**（写保护与策略检查都不再发生）。
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let escaped = cwd.join("..").join("..").join("etc").join("passwd");
    let call = GateCall {
        tool_name: "Write".to_string(),
        command: None,
        original_command: None,
        path: Some(escaped.clone()),
        branch: None,
        cwd: cwd.clone(),
    };
    assert!(
        g.deterministic(&call).is_none(),
        "穿越路径 {escaped:?} 不该走快车道，实际: {:?}",
        g.deterministic(&call)
    );
}

#[test]
fn test_write_inside_project_still_fast_lanes() {
    // 反向对照：真正的项目内路径仍应走快车道（别把上面那条改成一律不走快车道）
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Write".to_string(),
        command: None,
        original_command: None,
        path: Some(cwd.join("src").join("main.rs")),
        branch: None,
        cwd: cwd.clone(),
    };
    assert!(matches!(
        g.deterministic(&call),
        Some(GateDecision::Allow { .. })
    ));
}

#[test]
fn test_write_outside_project_escalates() {
    // 反向对照：项目外的绝对路径也不该走快车道
    let g = gate();
    let cwd = PathBuf::from(CWD);
    let call = GateCall {
        tool_name: "Write".to_string(),
        command: None,
        original_command: None,
        path: Some(PathBuf::from("/etc/passwd")),
        branch: None,
        cwd: cwd.clone(),
    };
    assert!(g.deterministic(&call).is_none());
}

#[test]
fn test_block_message_truncates_huge_policy() {
    // 策略可能上万字，不能整段灌进错误信息
    let g = gate_with(JevConfig {
        policy: "长".repeat(5000),
        ..Default::default()
    });
    let msg = g.block_message("policy_compliance");
    assert!(
        msg.chars().count() < 1500,
        "msg 过长: {}",
        msg.chars().count()
    );
}
