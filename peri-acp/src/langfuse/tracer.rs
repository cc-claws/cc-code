use std::{collections::HashMap, sync::Arc};

use langfuse_client::{GenerationBody, IngestionEvent, ObservationBody, ObservationType, SpanBody};
use peri_agent::{llm::types::TokenUsage, messages::BaseMessage, tools::ToolDefinition};

use super::session::LangfuseSession;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 工具调用的中间缓冲数据（start 时存储，end 时取出组合成完整 span-create）
pub(crate) struct PendingTool {
    span_id: String,
    name: String,
    input: serde_json::Value,
    start_time: String,
    /// 父 span ID（= 所属批次的 tools_batch_span_id）
    parent_span_id: String,
}

/// Langfuse 单轮追踪器（per-turn）
///
/// 持有对 LangfuseSession 的引用，复用 client/batcher。
/// 生命周期：从 execute_prompt 开始 → AgentEvent::Done/Error 时结束。
///
/// 所有事件通过 `batcher.try_add()` 同步入队，保证事件顺序与调用顺序一致，
/// 确保 Langfuse 层级关系正确（父 span 先于子 span 入队）。
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,
    /// Langfuse session_id = 会话的 thread_id，用于在 Langfuse UI 中按会话分组
    session_id: String,
    /// 当前对话轮次的 Trace ID（提前生成，所有观测对象共享）
    trace_id: String,
    /// 主 Agent Observation 的 ID
    pub(crate) agent_observation_id: String,
    /// step → (generation_id, input_messages, tools, start_time_rfc3339)
    generation_data: HashMap<usize, (String, Vec<BaseMessage>, Vec<ToolDefinition>, String)>,
    /// 工具调用缓冲数据：tool_call_id → PendingTool
    pending_tools: HashMap<String, PendingTool>,
    /// 当前批次工具组 Span ID
    tools_batch_span_id: Option<String>,
    /// 当前批次工具组开始时间
    tools_batch_start_time: Option<String>,
    /// 当前批次工具组最后一次 ToolEnd 时间
    tools_batch_end_time: Option<String>,
    /// 累积的最终回答
    final_answer: String,
    /// SubAgent 栈：保存当前活动的 subagent observation IDs
    /// 支持 subagent 嵌套调用（subagent 中再调用 subagent）
    pub(crate) subagent_stack: Vec<SubAgentContext>,
    /// Compact Span 上下文（非 None 表示正在 compact 操作中）
    compact_span: Option<CompactSpanContext>,
    /// 当前活跃的 LLM step 编号（用于将 LlmRetrying 关联到正确 generation）
    active_step: Option<usize>,
    /// 当前 step 的 LLM 重试记录（每次 on_llm_start 清空）
    retry_attempts: Vec<RetryAttempt>,
}

/// SubAgent 追踪上下文
pub(crate) struct SubAgentContext {
    /// SubAgent 的 Observation ID
    pub(crate) observation_id: String,
    /// SubAgent 的 agent_id（如 "code-reviewer"）
    pub(crate) agent_id: String,
    /// SubAgent 开始时间（延迟到 end_subagent 时与 ObservationCreate 一起发送）
    pub(crate) start_time: String,
    /// SubAgent 输入（prompt 预览）
    pub(crate) input: serde_json::Value,
    /// 当前 subagent 下的 tools batch 信息
    pub(crate) tools_batch_span_id: Option<String>,
    pub(crate) tools_batch_start_time: Option<String>,
    pub(crate) tools_batch_end_time: Option<String>,
    /// SubAgent 下的工具调用缓冲
    pub(crate) pending_tools: HashMap<String, PendingTool>,
}

/// Compact Span 上下文（CompactStarted → CompactCompleted/Error 期间存续）
pub(crate) struct CompactSpanContext {
    /// Compact Span 的 Observation ID
    pub(crate) span_id: String,
    /// Compact 开始时间
    pub(crate) start_time: String,
}

/// 单次 LLM 重试记录
pub(crate) struct RetryAttempt {
    pub(crate) attempt: usize,
    pub(crate) max_attempts: usize,
    pub(crate) delay_ms: u64,
    pub(crate) error: String,
}

impl LangfuseTracer {
    /// 从共享 Session 构造 per-turn Tracer
    pub fn new(session: Arc<LangfuseSession>, session_id: String) -> Self {
        Self {
            session,
            session_id,
            trace_id: uuid::Uuid::now_v7().to_string(),
            agent_observation_id: uuid::Uuid::now_v7().to_string(),
            generation_data: HashMap::new(),
            pending_tools: HashMap::new(),
            tools_batch_span_id: None,
            tools_batch_start_time: None,
            tools_batch_end_time: None,
            final_answer: String::new(),
            subagent_stack: Vec::new(),
            compact_span: None,
            active_step: None,
            retry_attempts: Vec::new(),
        }
    }

    /// 获取当前活动的 agent observation ID
    /// 若有 subagent 栈，返回栈顶的 subagent ID；否则返回主 agent ID
    pub(crate) fn current_agent_id(&self) -> String {
        self.subagent_stack
            .last()
            .map(|ctx| ctx.observation_id.clone())
            .unwrap_or_else(|| self.agent_observation_id.clone())
    }

    /// 获取当前活动的 tools batch 上下文
    fn current_tools_context(
        &mut self,
    ) -> (
        &mut Option<String>,
        &mut Option<String>,
        &mut Option<String>,
        &mut HashMap<String, PendingTool>,
    ) {
        if let Some(subagent) = self.subagent_stack.last_mut() {
            (
                &mut subagent.tools_batch_span_id,
                &mut subagent.tools_batch_start_time,
                &mut subagent.tools_batch_end_time,
                &mut subagent.pending_tools,
            )
        } else {
            (
                &mut self.tools_batch_span_id,
                &mut self.tools_batch_start_time,
                &mut self.tools_batch_end_time,
                &mut self.pending_tools,
            )
        }
    }

    /// TextChunk 事件：累积最终回答
    pub fn on_text_chunk(&mut self, chunk: &str) {
        self.final_answer.push_str(chunk);
    }

    /// 从 Agent 工具的输入 JSON 中提取 subagent 标识（用于 Langfuse 显示名称）
    pub(crate) fn subagent_identity(input: &serde_json::Value) -> String {
        input
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                input
                    .get("fork")
                    .and_then(|v| v.as_bool())
                    .filter(|&f| f)
                    .map(|_| "fork".to_string())
            })
            .unwrap_or_else(|| "fork".to_string())
    }

    /// 创建 SubAgent 上下文并压入 subagent_stack
    ///
    /// Observation 延迟到 end_subagent 时发送，确保与 Tools batch 在同一批次，
    /// 避免周期性 flush 导致 parent 缺失引发 Langfuse 重复 trace。
    fn begin_subagent(&mut self, input: &serde_json::Value) {
        let agent_id = Self::subagent_identity(input);
        let task_preview: String = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(200).collect())
            .unwrap_or_default();

        let observation_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        self.subagent_stack.push(SubAgentContext {
            observation_id,
            agent_id,
            start_time,
            input: serde_json::json!(task_preview),
            tools_batch_span_id: None,
            tools_batch_start_time: None,
            tools_batch_end_time: None,
            pending_tools: HashMap::new(),
        });
    }

    /// 完成当前 SubAgent Observation（Span 类型）：先发 ObservationCreate（确保 parent 先入队），
    /// 再 flush 工具批次，最后弹出栈。
    ///
    /// 必须在 `subagent_stack.pop()` 之前调用 `flush_tools_batch()`，
    /// 否则 subagent 的工具批次会 flush 到错误的 parent。
    fn end_subagent(&mut self, result: &str, is_error: bool) {
        // 先发 SubAgent ObservationCreate，再 flush Tools batch
        // 确保 Tools SpanCreate 的 parent（subagent observation）先于它入队
        let status_message = if is_error {
            Some("error".to_string())
        } else {
            None
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        if let Some(ctx) = self.subagent_stack.last() {
            let obs_body = ObservationBody {
                id: Some(ctx.observation_id.clone()),
                trace_id: Some(self.trace_id.clone()),
                r#type: ObservationType::Agent,
                name: Some(format!("subagent:{}", ctx.agent_id)),
                start_time: Some(ctx.start_time.clone()),
                end_time: Some(end_time.clone()),
                completion_start_time: None,
                parent_observation_id: None,
                input: Some(ctx.input.clone()),
                output: Some(serde_json::json!(result)),
                metadata: None,
                model: None,
                model_parameters: None,
                level: None,
                status_message,
                version: Some(VERSION.to_string()),
                environment: None,
                session_id: Some(self.session_id.clone()),
            };
            let obs_event = IngestionEvent::ObservationCreate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: end_time,
                body: obs_body,
                metadata: None,
            };
            if let Err(e) = self.session.batcher.try_add(obs_event) {
                tracing::warn!(
                    error = %e, trace_id = %self.trace_id, subagent = %ctx.agent_id,
                    "langfuse: subagent observation create 入队失败（背压丢弃）"
                );
            }
        }

        // flush subagent 下的 tools batch（pop 前）
        self.flush_tools_batch();

        if self.subagent_stack.pop().is_none() {
            tracing::warn!("langfuse: end_subagent 调用时 subagent_stack 为空，忽略");
        }
    }

    /// 提交当前批次 Tools Span
    fn flush_tools_batch(&mut self) {
        let (batch_id, batch_start, batch_end, parent_id) = {
            let (batch_id_ref, batch_start_ref, batch_end_ref, _) = self.current_tools_context();
            if let (Some(batch_id), Some(batch_start), Some(batch_end)) = (
                batch_id_ref.take(),
                batch_start_ref.take(),
                batch_end_ref.take(),
            ) {
                (batch_id, batch_start, batch_end, self.current_agent_id())
            } else {
                return;
            }
        };

        let body = SpanBody {
            id: Some(batch_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            name: Some("Tools".to_string()),
            start_time: Some(batch_start),
            end_time: Some(batch_end.clone()),
            parent_observation_id: Some(parent_id),
            input: None,
            output: None,
            status_message: None,
            metadata: None,
            level: None,
            version: Some(VERSION.to_string()),
            environment: None,
            session_id: Some(self.session_id.clone()),
        };
        let event = IngestionEvent::SpanCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: batch_end,
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: tools batch span 入队失败（背压丢弃）");
        }
    }

    /// 对话轮次开始：创建 agent-run Observation（根 observation）
    pub fn on_trace_start(&mut self, input: &str) {
        let batcher = &self.session.batcher;
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        tracing::info!(
            trace_id = %self.trace_id,
            agent_obs_id = %self.agent_observation_id,
            "langfuse: on_trace_start called"
        );

        // 创建 agent-run 根 Observation（OTLP 通过 trace_id 隐式创建 Trace，无需 TraceCreate）
        let body = ObservationBody {
            id: Some(self.agent_observation_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            r#type: ObservationType::Agent,
            name: Some("agent-run".to_string()),
            start_time: Some(start_time),
            end_time: None,
            completion_start_time: None,
            parent_observation_id: None,
            input: Some(serde_json::json!(input)),
            output: None,
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: None,
            version: Some(VERSION.to_string()),
            environment: None,
            session_id: Some(self.session_id.clone()),
        };
        let event = IngestionEvent::ObservationCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            body,
            metadata: None,
        };
        if let Err(e) = batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: agent-run observation 入队失败（背压丢弃）");
        }
    }

    /// LLM 调用开始：提交上一轮工具批次 Span，缓存本轮 input
    pub fn on_llm_start(
        &mut self,
        step: usize,
        messages: &[BaseMessage],
        tools: &[ToolDefinition],
    ) {
        self.flush_tools_batch();
        let gen_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.generation_data.insert(
            step,
            (gen_id, messages.to_vec(), tools.to_vec(), start_time),
        );
        self.active_step = Some(step);
        self.retry_attempts.clear();
    }

    /// LLM 调用结束：同步创建 Generation 事件
    pub fn on_llm_end(
        &mut self,
        step: usize,
        model: &str,
        provider: &str,
        output: &str,
        usage: Option<&TokenUsage>,
    ) {
        let Some((gen_id, messages, tools, start_time)) = self.generation_data.remove(&step) else {
            return;
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let messages_val = serde_json::to_value(&messages).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: messages 序列化失败");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let tools_val = serde_json::to_value(&tools).unwrap_or_else(|e| {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: tools 序列化失败");
            serde_json::json!({ "error": "serialization failed", "detail": e.to_string() })
        });
        let input_json = serde_json::json!({
            "messages": messages_val,
            "tools": tools_val,
        });

        let langfuse_usage_details: Option<HashMap<String, i32>> = usage.map(|u| {
            let mut map = HashMap::new();
            let cache_creation = u.cache_creation_input_tokens.unwrap_or(0);
            let cache_read = u.cache_read_input_tokens.unwrap_or(0);
            // input_tokens 已被适配器规范化（Anthropic: raw + cache_creation + cache_read），
            // Langfuse 要求 input 为不含缓存的原始值，需减去缓存部分。
            let raw_input = u.input_tokens.saturating_sub(cache_creation + cache_read);
            let total = raw_input + u.output_tokens + cache_creation + cache_read;
            map.insert("input".to_string(), raw_input as i32);
            map.insert("output".to_string(), u.output_tokens as i32);
            map.insert("total".to_string(), total as i32);
            if cache_creation > 0 {
                map.insert(
                    "cache_creation_input_tokens".to_string(),
                    cache_creation as i32,
                );
            }
            if cache_read > 0 {
                map.insert("cache_read_input_tokens".to_string(), cache_read as i32);
            }
            map
        });

        let gen_metadata = if self.retry_attempts.is_empty() {
            None
        } else {
            let retries: Vec<serde_json::Value> = self
                .retry_attempts
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "attempt": r.attempt,
                        "max_attempts": r.max_attempts,
                        "delay_ms": r.delay_ms,
                        "error": r.error,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "retry_count": self.retry_attempts.len(),
                "retries": retries,
            }))
        };
        self.active_step = None;
        self.retry_attempts.clear();

        let body = GenerationBody {
            id: Some(gen_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            name: Some(format!("Chat{}", provider)),
            input: Some(input_json),
            output: Some(serde_json::json!(output)),
            model: Some(model.to_string()),
            usage_details: langfuse_usage_details,
            parent_observation_id: Some(self.current_agent_id()),
            start_time: Some(start_time),
            end_time: Some(end_time.clone()),
            session_id: Some(self.session_id.clone()),
            version: Some(VERSION.to_string()),
            ..Default::default()
        };
        let event = IngestionEvent::GenerationCreate {
            id: gen_id.clone(),
            timestamp: end_time,
            body,
            metadata: gen_metadata,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, gen_id = %gen_id, "langfuse: generation 入队失败（背压丢弃）");
        }
    }

    /// 工具调用开始
    pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
        let is_agent = name == "Agent";
        let tool_span_id;

        // Block 限定 current_tools_context 的可变借用范围
        {
            let current_agent_id = self.current_agent_id();
            let (batch_id_ref, start_time_ref, _, pending_tools) = self.current_tools_context();
            if pending_tools.is_empty() {
                *batch_id_ref = Some(uuid::Uuid::now_v7().to_string());
                *start_time_ref =
                    Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            }
            let parent_span_id = batch_id_ref.clone().unwrap_or(current_agent_id);

            tool_span_id = uuid::Uuid::now_v7().to_string();
            let start_time =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            pending_tools.insert(
                tool_call_id.to_string(),
                PendingTool {
                    span_id: tool_span_id.clone(),
                    name: name.to_string(),
                    input: input.clone(),
                    start_time,
                    parent_span_id,
                },
            );
        } // 可变借用在此释放

        // Agent 工具：创建 SubAgent Span，push 到栈
        if is_agent {
            self.begin_subagent(input);
        }
    }

    /// 工具调用结束：同步创建 tool observation
    pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
        let session_id = self.session_id.clone();
        let trace_id = self.trace_id.clone();
        let trace_id_for_log = self.trace_id.clone();

        // Agent 工具的 PendingTool 在 on_tool_start 时插入到**父级** context
        //（begin_subagent push 前），而 current_tools_context() 会返回子 agent 的 context。
        // 因此必须先 end_subagent（pop 栈回到父级），再查找 PendingTool。
        let is_agent = self
            .pending_tools
            .get(tool_call_id)
            .map(|t| t.name == "Agent")
            .unwrap_or(false)
            || self.subagent_stack.iter().any(|ctx| {
                ctx.pending_tools
                    .get(tool_call_id)
                    .map(|t| t.name == "Agent")
                    .unwrap_or(false)
            });
        if is_agent {
            self.end_subagent(output, is_error);
        }

        let (_, _, end_time_ref, pending_tools) = self.current_tools_context();
        let Some(tool) = pending_tools.remove(tool_call_id) else {
            return;
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let tool_name = tool.name.clone();
        let span_id = tool.span_id;
        let tool_name_for_body = tool.name.clone();
        let tool_input = tool.input;
        let tool_start_time = tool.start_time;
        let tool_parent_id = tool.parent_span_id;

        let status_msg = if is_error {
            Some("error".to_string())
        } else {
            None
        };

        let body = ObservationBody {
            id: Some(span_id),
            trace_id: Some(trace_id),
            r#type: ObservationType::Tool,
            name: Some(tool_name_for_body),
            input: Some(tool_input),
            output: Some(serde_json::json!(output)),
            start_time: Some(tool_start_time),
            end_time: Some(end_time.clone()),
            completion_start_time: None,
            parent_observation_id: Some(tool_parent_id),
            metadata: None,
            model: None,
            model_parameters: None,
            level: None,
            status_message: status_msg,
            version: Some(VERSION.to_string()),
            environment: None,
            session_id: Some(session_id),
        };
        let event = IngestionEvent::ObservationCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: end_time.clone(),
            body,
            metadata: None,
        };
        // 释放 current_tools_context 的可变借用
        let _ = end_time_ref;
        let _ = pending_tools;

        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %trace_id_for_log, tool = %tool_name, "langfuse: tool observation 入队失败（背压丢弃）");
        }

        // 重新获取可变借用
        let (_, _, end_time_ref, _) = self.current_tools_context();
        *end_time_ref = Some(end_time);
    }

    /// LLM 重试：记录重试信息，最终在 on_llm_end 时写入 Generation metadata
    pub fn on_llm_retrying(
        &mut self,
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: &str,
    ) {
        self.retry_attempts.push(RetryAttempt {
            attempt,
            max_attempts,
            delay_ms,
            error: error.to_string(),
        });
    }

    /// Compact 开始：创建 compact Span（子 span 挂载到当前 agent observation）
    pub fn on_compact_start(&mut self) {
        let span_id = uuid::Uuid::now_v7().to_string();
        let start_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let body = SpanBody {
            id: Some(span_id.clone()),
            trace_id: Some(self.trace_id.clone()),
            name: Some("compact".to_string()),
            start_time: Some(start_time.clone()),
            end_time: None,
            parent_observation_id: Some(self.current_agent_id()),
            input: None,
            output: None,
            status_message: None,
            metadata: None,
            level: None,
            version: Some(VERSION.to_string()),
            environment: None,
            session_id: Some(self.session_id.clone()),
        };
        let event = IngestionEvent::SpanCreate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: start_time.clone(),
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: compact span 入队失败（背压丢弃）");
        }

        self.compact_span = Some(CompactSpanContext {
            span_id,
            start_time,
        });
    }

    /// Compact 完成/错误：更新 compact Span 的 output + end_time（或 error status）
    ///
    /// `summary`: full compact 时为摘要文本，micro compact 时为空
    /// `files_count`: 保留的文件数量
    /// `skills_count`: 保留的 Skill 数量
    /// `micro_cleared`: >0 表示 micro compact（清除的工具结果数）
    /// `is_error`: 是否为压缩失败
    /// `error_message`: 失败时的错误信息
    pub fn on_compact_end(
        &mut self,
        summary: &str,
        files_count: usize,
        skills_count: usize,
        micro_cleared: usize,
        is_error: bool,
        error_message: &str,
    ) {
        let Some(ctx) = self.compact_span.take() else {
            return;
        };
        let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let compact_type = if micro_cleared > 0 { "micro" } else { "full" };
        let summary_preview: String = summary.chars().take(200).collect();

        let output = if is_error {
            serde_json::json!({
                "type": compact_type,
                "error": error_message,
            })
        } else if micro_cleared > 0 {
            serde_json::json!({
                "type": compact_type,
                "micro_cleared": micro_cleared,
            })
        } else {
            serde_json::json!({
                "type": compact_type,
                "summary": summary_preview,
                "files_count": files_count,
                "skills_count": skills_count,
            })
        };

        let status_message = if is_error {
            Some(if error_message.is_empty() {
                "error".to_string()
            } else {
                error_message.to_string()
            })
        } else {
            None
        };

        let body = SpanBody {
            id: Some(ctx.span_id),
            trace_id: Some(self.trace_id.clone()),
            name: Some("compact".to_string()),
            start_time: Some(ctx.start_time),
            end_time: Some(end_time.clone()),
            parent_observation_id: Some(self.current_agent_id()),
            input: None,
            output: Some(output),
            status_message,
            metadata: if !is_error && micro_cleared == 0 {
                Some(serde_json::json!({
                    "summary_full": summary,
                    "files_count": files_count,
                    "skills_count": skills_count,
                }))
            } else {
                None
            },
            level: None,
            version: Some(VERSION.to_string()),
            environment: None,
            session_id: Some(self.session_id.clone()),
        };
        let event = IngestionEvent::SpanUpdate {
            id: uuid::Uuid::now_v7().to_string(),
            timestamp: end_time,
            body,
            metadata: None,
        };
        if let Err(e) = self.session.batcher.try_add(event) {
            tracing::warn!(error = %e, trace_id = %self.trace_id, "langfuse: compact span 更新入队失败（背压丢弃）");
        }
    }

    /// 对话轮次结束：更新 agent-run Observation 输出和结束时间，并强制 flush。
    pub fn on_trace_end(&mut self, error_output: Option<&str>) -> tokio::task::JoinHandle<()> {
        self.flush_tools_batch();

        let batcher = Arc::clone(&self.session.batcher);
        let trace_id = self.trace_id.clone();
        let agent_observation_id = self.agent_observation_id.clone();
        let output = if let Some(err) = error_output {
            err.to_string()
        } else {
            std::mem::take(&mut self.final_answer)
        };

        tokio::spawn(async move {
            let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

            // 更新 agent-run Observation 的 output 和 end_time
            let obs_body = ObservationBody {
                id: Some(agent_observation_id.clone()),
                trace_id: Some(trace_id.clone()),
                r#type: ObservationType::Agent,
                name: Some("agent-run".to_string()),
                output: Some(serde_json::json!(output)),
                end_time: Some(end_time.clone()),
                version: Some(VERSION.to_string()),
                ..Default::default()
            };
            let obs_event = IngestionEvent::ObservationUpdate {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: end_time,
                body: obs_body,
                metadata: None,
            };
            if let Err(e) = batcher.add(obs_event).await {
                tracing::warn!(error = %e, trace_id = %trace_id, obs_id = %agent_observation_id, "langfuse: agent-run observation 更新失败");
            }
            if let Err(e) = batcher.flush().await {
                tracing::warn!(error = %e, trace_id = %trace_id, "langfuse: batcher flush 失败");
            }
        })
    }
}

#[cfg(test)]
#[path = "tracer_test.rs"]
mod tests;
