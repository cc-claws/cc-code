use super::*;

fn make_openai_provider(model: &str) -> LlmProvider {
    LlmProvider::OpenAi {
        api_key: "test-key".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        model: model.to_string(),
        thinking: None,
    }
}

fn make_anthropic_provider(model: &str) -> LlmProvider {
    LlmProvider::Anthropic {
        api_key: "test-key".to_string(),
        model: model.to_string(),
        base_url: None,
        thinking: None,
    }
}

#[test]
fn test_agent_pool_new_is_empty() {
    let pool = AgentPool::new();
    assert!(pool.get_cached_llm().is_none());
    assert!(pool.fingerprint().is_empty());
}

#[test]
fn test_has_valid_cache_empty_pool() {
    let pool = AgentPool::new();
    let provider = make_openai_provider("gpt-4o");
    assert!(!pool.has_valid_cache(&provider));
}

#[test]
fn test_invalidate_clears_cache() {
    let mut pool = AgentPool::new();
    pool.fingerprint = "OpenAI:gpt-4o".to_string();
    pool.invalidate();
    assert!(pool.get_cached_llm().is_none());
    assert!(pool.fingerprint().is_empty());
}

#[test]
fn test_has_valid_cache_fingerprint_mismatch() {
    let mut pool = AgentPool::new();
    // 模拟已缓存但 fingerprint 不匹配
    pool.fingerprint = "OpenAI:gpt-4o".to_string();
    // cached_llm 为 None，has_valid_cache 应返回 false
    let provider = make_openai_provider("gpt-4o");
    assert!(!pool.has_valid_cache(&provider));
}

#[test]
fn test_fingerprint_openai() {
    let provider = make_openai_provider("gpt-4o-mini");
    let fp = fingerprint(&provider);
    assert_eq!(fp, "OpenAI:gpt-4o-mini");
}

#[test]
fn test_fingerprint_anthropic() {
    let provider = make_anthropic_provider("claude-sonnet-4-20250514");
    let fp = fingerprint(&provider);
    assert_eq!(fp, "Anthropic:claude-sonnet-4-20250514");
}

#[test]
fn test_has_valid_cache_after_fingerprint_only_set() {
    let mut pool = AgentPool::new();
    // 直接设置 fingerprint 但没有 cached_llm
    pool.fingerprint = "OpenAI:gpt-4o".to_string();
    let provider = make_openai_provider("gpt-4o");
    // cached_llm 为 None → false
    assert!(!pool.has_valid_cache(&provider));
}

#[test]
fn test_invalidate_clears_subagent_cache() {
    let mut pool = AgentPool::new();
    // 模拟 subagent_llm_cache 中有数据
    pool.subagent_llm_cache.insert(
        "OpenAI:gpt-4o".to_string(),
        Arc::new(mock_base_model("gpt-4o")),
    );
    assert!(!pool.subagent_llm_cache.is_empty());

    pool.invalidate();
    // invalidate 应同时清空 subagent_llm_cache
    assert!(pool.subagent_llm_cache.is_empty());
    assert!(pool.get_cached_llm().is_none());
    assert!(pool.fingerprint().is_empty());
}

#[test]
fn test_subagent_cache_miss_creates_new() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    // 首次查询 → 缓存未命中 → 创建新实例
    let model =
        AgentPool::get_or_create_subagent_llm(&Arc::downgrade(&pool), "OpenAI:gpt-4o", || {
            Box::new(mock_base_model("gpt-4o"))
        });
    assert_eq!(model.model_id(), "gpt-4o");
    assert_eq!(model.provider_name(), "Mock");
}

#[test]
fn test_subagent_cache_hit_returns_same() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let m1 = AgentPool::get_or_create_subagent_llm(&Arc::downgrade(&pool), "OpenAI:gpt-4o", || {
        Box::new(mock_base_model("gpt-4o"))
    });
    let m2 = AgentPool::get_or_create_subagent_llm(&Arc::downgrade(&pool), "OpenAI:gpt-4o", || {
        Box::new(mock_base_model("gpt-4o"))
    });
    // 相同 fingerprint → 返回同一个 Arc（ptr_eq）
    assert!(Arc::ptr_eq(&m1, &m2));
}

#[test]
fn test_subagent_cache_different_fingerprint_isolation() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let m1 = AgentPool::get_or_create_subagent_llm(&Arc::downgrade(&pool), "OpenAI:gpt-4o", || {
        Box::new(mock_base_model("gpt-4o"))
    });
    let m2 =
        AgentPool::get_or_create_subagent_llm(&Arc::downgrade(&pool), "OpenAI:gpt-4o-mini", || {
            Box::new(mock_base_model("gpt-4o-mini"))
        });
    assert_ne!(m1.model_id(), m2.model_id());
    assert!(!Arc::ptr_eq(&m1, &m2));
}

#[test]
fn test_subagent_cache_creates_model_after_pool_release() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let weak = Arc::downgrade(&pool);
    drop(pool);
    let first = AgentPool::get_or_create_subagent_llm(&weak, "OpenAI:gpt-4o", || {
        Box::new(mock_base_model("gpt-4o"))
    });
    let second = AgentPool::get_or_create_subagent_llm(&weak, "OpenAI:gpt-4o", || {
        Box::new(mock_base_model("gpt-4o"))
    });
    assert_eq!(first.model_id(), "gpt-4o", "会话释放后后台任务仍可构造模型");
    assert!(!Arc::ptr_eq(&first, &second), "会话释放后不再保留模型缓存");
    assert!(weak.upgrade().is_none(), "模型构造不能延长会话池生命周期");
}

struct MockFactoryTool {
    factory: Arc<dyn Fn() -> Arc<dyn BaseModel> + Send + Sync>,
}

#[async_trait::async_trait]
impl cc_agent::tools::BaseTool for MockFactoryTool {
    fn name(&self) -> &str {
        "Agent"
    }
    fn description(&self) -> &str {
        "持有模型工厂的测试工具"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok((self.factory)().model_id().to_string())
    }
}

#[test]
fn test_subagent_factory_does_not_retain_pool_through_tool_registry() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let weak_pool = Arc::downgrade(&pool);
    let pool_for_factory = Arc::downgrade(&pool);
    let factory: Arc<dyn Fn() -> Arc<dyn BaseModel> + Send + Sync> = Arc::new(move || {
        AgentPool::get_or_create_subagent_llm(&pool_for_factory, "OpenAI:gpt-4o", || {
            Box::new(mock_base_model("gpt-4o"))
        })
    });
    let tool: Arc<dyn cc_agent::tools::BaseTool> = Arc::new(MockFactoryTool {
        factory: Arc::clone(&factory),
    });
    let weak_tool = Arc::downgrade(&tool);
    let tools = Arc::new(parking_lot::RwLock::new(HashMap::from([(
        "Agent".into(),
        tool,
    )])));
    let weak_tools = Arc::downgrade(&tools);
    pool.lock().project_tools = Some(ProjectToolResources {
        cwd: ".".into(),
        mcp_pool: None,
        index: Arc::default(),
        tools,
    });
    let first = factory();
    let cached = factory();
    assert!(Arc::ptr_eq(&first, &cached), "会话存活时必须继续复用缓存");
    drop(pool);
    assert!(
        weak_pool.upgrade().is_none(),
        "工具中的模型工厂不能反向持有会话池"
    );
    assert!(
        weak_tools.upgrade().is_none(),
        "会话释放后必须释放工具注册表"
    );
    assert!(weak_tool.upgrade().is_none(), "注册表释放后必须析构工具");
    assert_eq!(factory().model_id(), "gpt-4o", "后台持有的工厂仍可创建模型");
}

// 简单的 mock BaseModel 用于测试
fn mock_base_model(name: &str) -> impl BaseModel {
    use async_trait::async_trait;
    use cc_agent::{
        llm::{LlmRequest, LlmResponse, StopReason},
        messages::BaseMessage,
    };

    struct MockModel {
        name: String,
    }

    #[async_trait]
    impl BaseModel for MockModel {
        async fn invoke(&self, _request: LlmRequest) -> cc_agent::error::AgentResult<LlmResponse> {
            Ok(LlmResponse {
                message: BaseMessage::ai("mock response"),
                stop_reason: StopReason::EndTurn,
                usage: None,
                request_id: None,
            })
        }

        fn model_id(&self) -> &str {
            &self.name
        }

        fn provider_name(&self) -> &str {
            "Mock"
        }
    }

    MockModel {
        name: name.to_string(),
    }
}
