use anix_domain::{ContextSnapshotId, Error, Result, TokenUsage, WorkBudget, WorkNodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

pub type IntelligenceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroTask {
    Classification,
    QueryRewrite,
    CandidateExtraction,
    Tagging,
    ShortSummary,
    ContextCompression,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    pub structured_output: bool,
    pub streaming: bool,
    pub context_tokens: u32,
    pub tasks: BTreeSet<MicroTask>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeRequest {
    pub instruction: String,
    pub input: String,
    pub schema: Value,
    pub max_output_tokens: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeResponse {
    pub output: Value,
    pub usage: TokenUsage,
    pub model: String,
    pub revision: String,
    pub runtime: String,
    pub timings: Option<RuntimeTimings>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeTimings {
    pub prompt_tokens: u64,
    pub prompt_ms: f64,
    pub generated_tokens: u64,
    pub generation_ms: f64,
}
/// Portable runtime port. No framework, HTTP client or model brand enters Core.
pub trait ModelRuntime: Send + Sync {
    fn capabilities(&self) -> RuntimeCapabilities;
    fn available(&self) -> bool;
    fn generate<'a>(&'a self, request: RuntimeRequest) -> IntelligenceFuture<'a, RuntimeResponse>;
    fn generate_structured<'a>(
        &'a self,
        request: RuntimeRequest,
    ) -> IntelligenceFuture<'a, RuntimeResponse> {
        self.generate(request)
    }
    fn stream<'a>(&'a self, request: RuntimeRequest) -> IntelligenceFuture<'a, Vec<String>> {
        Box::pin(async move {
            if !self.capabilities().streaming {
                return Err(Error::Model("streaming unsupported".into()));
            }
            Ok(vec![self.generate(request).await?.output.to_string()])
        })
    }
}
pub struct FakeModelRuntime {
    pub responses: Mutex<VecDeque<Result<RuntimeResponse>>>,
    pub requests: Mutex<Vec<RuntimeRequest>>,
    pub enabled: AtomicBool,
}
impl FakeModelRuntime {
    pub fn new(responses: Vec<Result<RuntimeResponse>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(vec![]),
            enabled: AtomicBool::new(true),
        }
    }
}
impl ModelRuntime for FakeModelRuntime {
    fn available(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            structured_output: true,
            streaming: false,
            context_tokens: 8192,
            tasks: all_micro_tasks(),
        }
    }
    fn generate<'a>(&'a self, r: RuntimeRequest) -> IntelligenceFuture<'a, RuntimeResponse> {
        Box::pin(async move {
            self.requests
                .lock()
                .map_err(|_| Error::Model("fake request lock".into()))?
                .push(r);
            self.responses
                .lock()
                .map_err(|_| Error::Model("fake response lock".into()))?
                .pop_front()
                .unwrap_or_else(|| Err(Error::Model("fake exhausted".into())))
        })
    }
}
pub fn all_micro_tasks() -> BTreeSet<MicroTask> {
    [
        MicroTask::Classification,
        MicroTask::QueryRewrite,
        MicroTask::CandidateExtraction,
        MicroTask::Tagging,
        MicroTask::ShortSummary,
        MicroTask::ContextCompression,
    ]
    .into()
}
pub fn minimum_micro_quality(task: MicroTask) -> f64 {
    match task {
        MicroTask::Classification | MicroTask::Tagging => 0.97,
        MicroTask::CandidateExtraction => 0.95,
        MicroTask::QueryRewrite | MicroTask::ShortSummary | MicroTask::ContextCompression => 1.0,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassificationRequest {
    pub text: String,
    pub labels: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Classification {
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RewriteRequest {
    pub query: String,
    pub mandatory_terms: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RewrittenQuery {
    pub query: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractionRequest {
    pub text: String,
    pub maximum_candidates: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExtractedCandidate {
    pub text: String,
    pub source_quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExtractedCandidates {
    pub candidates: Vec<ExtractedCandidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaggingRequest {
    pub text: String,
    pub allowed_tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tags {
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SummaryRequest {
    pub text: String,
    pub mandatory_facts: Vec<String>,
    pub maximum_characters: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShortSummary {
    pub summary: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompressedContext {
    pub context: String,
}
/// Micro intelligence has no Core, AgentApi or Storage handle and cannot mutate science.
pub trait LocalIntelligence: Send + Sync {
    fn classify<'a>(&'a self, r: ClassificationRequest) -> IntelligenceFuture<'a, Classification>;
    fn rewrite_query<'a>(&'a self, r: RewriteRequest) -> IntelligenceFuture<'a, RewrittenQuery>;
    fn extract_candidates<'a>(
        &'a self,
        r: ExtractionRequest,
    ) -> IntelligenceFuture<'a, ExtractedCandidates>;
    fn tag<'a>(&'a self, r: TaggingRequest) -> IntelligenceFuture<'a, Tags>;
    fn summarize_short<'a>(&'a self, r: SummaryRequest) -> IntelligenceFuture<'a, ShortSummary>;
    fn compress_context<'a>(
        &'a self,
        r: SummaryRequest,
    ) -> IntelligenceFuture<'a, CompressedContext>;
}
pub trait PromptTemplates: Send + Sync {
    fn instruction(&self, task: MicroTask) -> String;
    fn version(&self) -> &str;
}
pub struct RuntimeIntelligence {
    pub runtime: Arc<dyn ModelRuntime>,
    pub templates: Arc<dyn PromptTemplates>,
}
impl RuntimeIntelligence {
    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        task: MicroTask,
        input: Value,
        schema: Value,
    ) -> Result<T> {
        if !self.runtime.available() || !self.runtime.capabilities().tasks.contains(&task) {
            return Err(Error::Model(
                "local runtime unavailable or unsupported".into(),
            ));
        }
        let result = self
            .runtime
            .generate_structured(RuntimeRequest {
                instruction: self.templates.instruction(task),
                input: input.to_string(),
                schema,
                max_output_tokens: 512,
            })
            .await?;
        serde_json::from_value(result.output).map_err(|e| Error::Model(format!("schema: {e}")))
    }
}
pub fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn task_schema(task: MicroTask, input: &Value) -> Value {
    match task {
        MicroTask::Classification => object_schema(
            json!({"label":{"type":"string","enum":input["labels"]}}),
            &["label"],
        ),
        MicroTask::QueryRewrite => object_schema(json!({"query":{"type":"string"}}), &["query"]),
        MicroTask::CandidateExtraction => object_schema(
            json!({"candidates":{"type":"array","items":object_schema(json!({"text":{"type":"string"},"source_quote":{"type":"string"}}),&["text","source_quote"])}}),
            &["candidates"],
        ),
        MicroTask::Tagging => object_schema(
            json!({"tags":{"type":"array","items":{"type":"string","enum":input["allowed_tags"]},"uniqueItems":true}}),
            &["tags"],
        ),
        MicroTask::ShortSummary => {
            object_schema(json!({"summary":{"type":"string"}}), &["summary"])
        }
        MicroTask::ContextCompression => {
            object_schema(json!({"context":{"type":"string"}}), &["context"])
        }
    }
}
pub fn validate_micro(task: MicroTask, input: &Value, output: &Value) -> Result<()> {
    if serde_json::to_vec(output)
        .map_err(|e| Error::Model(e.to_string()))?
        .len()
        > 64 * 1024
    {
        return Err(Error::Validation("local output exceeds 64 KiB".into()));
    }
    let schema = task_schema(task, input);
    validate_schema(output, &schema)?;
    let failure = || Error::Validation("local output constraints failed".into());
    match task {
        MicroTask::Classification => {
            let r: Classification =
                serde_json::from_value(output.clone()).map_err(|_| failure())?;
            let i: ClassificationRequest =
                serde_json::from_value(input.clone()).map_err(|_| failure())?;
            if !i.labels.contains(&r.label) {
                return Err(failure());
            }
        }
        MicroTask::QueryRewrite => {
            let r: RewrittenQuery =
                serde_json::from_value(output.clone()).map_err(|_| failure())?;
            let i: RewriteRequest = serde_json::from_value(input.clone()).map_err(|_| failure())?;
            if r.query.trim().is_empty()
                || r.query.len() > 4096
                || i.mandatory_terms.iter().any(|t| !r.query.contains(t))
            {
                return Err(failure());
            }
        }
        MicroTask::CandidateExtraction => {
            let r: ExtractedCandidates =
                serde_json::from_value(output.clone()).map_err(|_| failure())?;
            let i: ExtractionRequest =
                serde_json::from_value(input.clone()).map_err(|_| failure())?;
            if r.candidates.len() > i.maximum_candidates as usize
                || r.candidates.iter().any(|c| {
                    c.text.trim().is_empty()
                        || !i.text.contains(&c.source_quote)
                        || !c.source_quote.contains(&c.text)
                })
            {
                return Err(failure());
            }
        }
        MicroTask::Tagging => {
            let r: Tags = serde_json::from_value(output.clone()).map_err(|_| failure())?;
            let i: TaggingRequest = serde_json::from_value(input.clone()).map_err(|_| failure())?;
            if r.tags.iter().any(|t| !i.allowed_tags.contains(t))
                || r.tags.iter().collect::<BTreeSet<_>>().len() != r.tags.len()
            {
                return Err(failure());
            }
        }
        MicroTask::ShortSummary | MicroTask::ContextCompression => {
            let key = if task == MicroTask::ShortSummary {
                "summary"
            } else {
                "context"
            };
            let t = output[key].as_str().ok_or_else(failure)?;
            let i: SummaryRequest = serde_json::from_value(input.clone()).map_err(|_| failure())?;
            if t.trim().is_empty()
                || t.chars().count() > i.maximum_characters as usize
                || i.mandatory_facts.iter().any(|f| !t.contains(f))
            {
                return Err(failure());
            }
        }
    }
    Ok(())
}
/// Small deterministic JSON Schema subset used by built-in contracts; rejects unsupported schemas.
pub fn validate_schema(value: &Value, schema: &Value) -> Result<()> {
    let bad = || Error::Validation("schema failure".into());
    match schema["type"].as_str() {
        Some("object") => {
            let obj = value.as_object().ok_or_else(bad)?;
            let props = schema["properties"].as_object().ok_or_else(bad)?;
            for key in schema["required"].as_array().ok_or_else(bad)? {
                if !obj.contains_key(key.as_str().ok_or_else(bad)?) {
                    return Err(bad());
                }
            }
            for (key, v) in obj {
                let Some(s) = props.get(key) else {
                    if schema["additionalProperties"] == false {
                        return Err(bad());
                    }
                    continue;
                };
                validate_schema(v, s)?;
            }
        }
        Some("array") => {
            let array = value.as_array().ok_or_else(bad)?;
            if schema["uniqueItems"] == true
                && array
                    .iter()
                    .map(Value::to_string)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != array.len()
            {
                return Err(bad());
            }
            for v in array {
                validate_schema(v, &schema["items"])?;
            }
        }
        Some("string") => {
            let Some(text) = value.as_str() else {
                return Err(bad());
            };
            if schema["maxLength"]
                .as_u64()
                .is_some_and(|max| text.chars().count() > max as usize)
            {
                return Err(bad());
            }
        }
        Some("integer") => {
            if value.as_i64().is_none() {
                return Err(bad());
            }
        }
        Some("boolean") => {
            if !value.is_boolean() {
                return Err(bad());
            }
        }
        _ => return Err(bad()),
    }
    if let Some(allowed) = schema["enum"].as_array() {
        if !allowed.contains(value) {
            return Err(bad());
        }
    }
    Ok(())
}
macro_rules! local_method {
    ($method:ident,$request:ty,$result:ty,$task:ident) => {
        fn $method<'a>(&'a self, r: $request) -> IntelligenceFuture<'a, $result> {
            Box::pin(async move {
                let input = serde_json::to_value(r).map_err(|e| Error::Model(e.to_string()))?;
                let result: $result = self
                    .call(
                        MicroTask::$task,
                        input.clone(),
                        task_schema(MicroTask::$task, &input),
                    )
                    .await?;
                validate_micro(
                    MicroTask::$task,
                    &input,
                    &serde_json::to_value(&result).map_err(|e| Error::Model(e.to_string()))?,
                )?;
                Ok(result)
            })
        }
    };
}
impl LocalIntelligence for RuntimeIntelligence {
    local_method!(
        classify,
        ClassificationRequest,
        Classification,
        Classification
    );
    local_method!(rewrite_query, RewriteRequest, RewrittenQuery, QueryRewrite);
    local_method!(
        extract_candidates,
        ExtractionRequest,
        ExtractedCandidates,
        CandidateExtraction
    );
    local_method!(tag, TaggingRequest, Tags, Tagging);
    local_method!(summarize_short, SummaryRequest, ShortSummary, ShortSummary);
    local_method!(
        compress_context,
        SummaryRequest,
        CompressedContext,
        ContextCompression
    );
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReasoningLevel {
    Simple,
    Complex,
    HighImpact,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImpactClass {
    LowRisk,
    Consequential,
    HighImpact,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRequirements {
    pub task_kind: String,
    pub micro_task: Option<MicroTask>,
    pub reasoning: ReasoningLevel,
    pub impact: ImpactClass,
    pub context_tokens: u32,
    pub structured_output: bool,
    pub required_capabilities: Vec<String>,
    pub private: bool,
    pub latency_budget_ms: u64,
    pub quality_threshold: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EscalationReason {
    HighImpact,
    ComplexReasoning,
    AmbiguousInput,
    SchemaFailure,
    ConstraintFailure,
    ContextOverflow,
    LocalRuntimeUnavailable,
    UnsupportedCapability,
    LowModelScore,
    RetryExhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Route {
    Deterministic,
    Local,
    Strong,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutingTelemetry {
    pub task_kind: String,
    pub route: Route,
    pub reason: Option<EscalationReason>,
    pub runtime: String,
    pub latency_ms: u64,
    pub usage: TokenUsage,
    pub validation_passed: bool,
    pub escalations: Vec<EscalationReason>,
}
#[derive(Default)]
pub struct CapabilityRegistry {
    pub providers: BTreeMap<String, BTreeSet<String>>,
    pub local_quality: BTreeMap<MicroTask, f64>,
}
impl CapabilityRegistry {
    pub fn supports(&self, provider: &str, required: &[String]) -> bool {
        self.providers
            .get(provider)
            .is_some_and(|set| required.iter().all(|c| set.contains(c)))
    }
}
pub trait DeterministicOperations: Send + Sync {
    fn execute(&self, task: &str, input: &Value) -> Option<Result<Value>>;
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoutingUsage {
    pub local_input: u64,
    pub local_output: u64,
    pub cloud_input: u64,
    pub cloud_output: u64,
    pub deterministic_operations: u64,
    pub estimated_cloud_tokens_avoided: u64,
    pub avoided_tokens_are_estimates: bool,
}
pub struct IntelligenceRouter {
    pub deterministic: Arc<dyn DeterministicOperations>,
    pub local: Arc<dyn ModelRuntime>,
    pub strong: Arc<dyn ModelRuntime>,
    pub templates: Arc<dyn PromptTemplates>,
    pub registry: CapabilityRegistry,
    pub telemetry: Mutex<Vec<RoutingTelemetry>>,
    pub usage: Mutex<RoutingUsage>,
}
impl IntelligenceRouter {
    pub async fn execute(
        &self,
        requirements: &TaskRequirements,
        input: Value,
        strong_schema: Value,
    ) -> Result<Value> {
        let start = Instant::now();
        if !requirements.quality_threshold.is_finite()
            || !(0.0..=1.0).contains(&requirements.quality_threshold)
        {
            return Err(Error::Validation("invalid quality threshold".into()));
        }
        if let Some(value) = self.deterministic.execute(&requirements.task_kind, &input) {
            let value = value?;
            self.log(
                requirements,
                Route::Deterministic,
                None,
                TokenUsage::default(),
                start,
                true,
                vec![],
            )?;
            return Ok(value);
        }
        let mut reasons = vec![];
        let forbidden = [
            "scientific_conclusion",
            "hypothesis_status",
            "experiment_design",
            "gate_decision",
            "final_synthesis",
            "statistical_interpretation",
            "plan_approval",
            "canonical_claim",
        ];
        let mut reason = if requirements.impact != ImpactClass::LowRisk
            || requirements.reasoning == ReasoningLevel::HighImpact
            || forbidden.contains(&requirements.task_kind.as_str())
        {
            Some(EscalationReason::HighImpact)
        } else if requirements.reasoning == ReasoningLevel::Complex {
            Some(EscalationReason::ComplexReasoning)
        } else if requirements.micro_task.is_none()
            || requirements.micro_task.is_some_and(|task| {
                serde_json::to_value(task)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .as_deref()
                    != Some(requirements.task_kind.as_str())
            })
        {
            Some(EscalationReason::UnsupportedCapability)
        } else if requirements.micro_task.is_some_and(|task| {
            self.registry.local_quality.get(&task).is_none_or(|score| {
                *score
                    < requirements
                        .quality_threshold
                        .max(minimum_micro_quality(task))
            })
        }) {
            Some(EscalationReason::LowModelScore)
        } else if !self.local.available() {
            Some(EscalationReason::LocalRuntimeUnavailable)
        } else if requirements.context_tokens > self.local.capabilities().context_tokens {
            Some(EscalationReason::ContextOverflow)
        } else if !self
            .registry
            .supports("local", &requirements.required_capabilities)
            || !self.local.capabilities().structured_output
        {
            Some(EscalationReason::UnsupportedCapability)
        } else {
            None
        };
        if reason.is_none() {
            let task = requirements
                .micro_task
                .ok_or_else(|| Error::Model("missing micro task".into()))?;
            if !self.local.capabilities().tasks.contains(&task) {
                reason = Some(EscalationReason::UnsupportedCapability);
            } else {
                for attempt in 0..2 {
                    let response = self
                        .local
                        .generate_structured(RuntimeRequest {
                            instruction: self.templates.instruction(task),
                            input: input.to_string(),
                            schema: task_schema(task, &input),
                            max_output_tokens: 512,
                        })
                        .await;
                    match response {
                        Ok(r) => {
                            let valid = validate_micro(task, &input, &r.output).and_then(|_| {
                                if start.elapsed().as_millis()
                                    > u128::from(requirements.latency_budget_ms)
                                {
                                    Err(Error::Validation("local latency budget exceeded".into()))
                                } else {
                                    Ok(())
                                }
                            });
                            self.log(
                                requirements,
                                Route::Local,
                                None,
                                r.usage.clone(),
                                start,
                                valid.is_ok(),
                                reasons.clone(),
                            )?;
                            if valid.is_ok() {
                                return Ok(r.output);
                            }
                            reasons.push(
                                if validate_schema(&r.output, &task_schema(task, &input)).is_err() {
                                    EscalationReason::SchemaFailure
                                } else {
                                    EscalationReason::ConstraintFailure
                                },
                            );
                        }
                        Err(_) => {
                            reasons.push(EscalationReason::LocalRuntimeUnavailable);
                            if attempt == 1 {
                                reasons.push(EscalationReason::RetryExhausted);
                            }
                        }
                    }
                    if attempt == 1 {
                        reasons.push(EscalationReason::RetryExhausted);
                    }
                }
                reason = reasons.last().copied();
            }
        }
        if let Some(r) = reason {
            reasons.push(r);
        }
        if requirements.private {
            return Err(Error::PermissionDenied);
        }
        if !self.strong.available() {
            return Err(Error::Model("strong runtime unavailable".into()));
        }
        let r = self
            .strong
            .generate_structured(RuntimeRequest {
                instruction: format!(
                    "Task: {}. Return JSON matching the supplied schema.",
                    requirements.task_kind
                ),
                input: input.to_string(),
                schema: strong_schema.clone(),
                max_output_tokens: 1024,
            })
            .await?;
        let validation = validate_schema(&r.output, &strong_schema).and_then(|_| {
            if let Some(task) = requirements.micro_task.filter(|task| {
                serde_json::to_value(task)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .as_deref()
                    == Some(requirements.task_kind.as_str())
            }) {
                validate_micro(task, &input, &r.output)
            } else {
                Ok(())
            }
        });
        self.log(
            requirements,
            Route::Strong,
            reason,
            r.usage.clone(),
            start,
            validation.is_ok(),
            reasons,
        )?;
        validation?;
        Ok(r.output)
    }
    #[allow(clippy::too_many_arguments)] // Internal telemetry includes independent usage, validation and escalation fields.
    fn log(
        &self,
        req: &TaskRequirements,
        route: Route,
        reason: Option<EscalationReason>,
        usage: TokenUsage,
        start: Instant,
        valid: bool,
        escalations: Vec<EscalationReason>,
    ) -> Result<()> {
        let mut aggregate = self
            .usage
            .lock()
            .map_err(|_| Error::Model("usage lock".into()))?;
        aggregate.avoided_tokens_are_estimates = true;
        match route {
            Route::Deterministic => {
                aggregate.deterministic_operations += 1;
                aggregate.estimated_cloud_tokens_avoided += u64::from(req.context_tokens);
            }
            Route::Local => {
                aggregate.local_input += usage.input;
                aggregate.local_output += usage.output;
                if valid {
                    aggregate.estimated_cloud_tokens_avoided +=
                        u64::from(req.context_tokens) + usage.output;
                }
            }
            Route::Strong => {
                aggregate.cloud_input += usage.input;
                aggregate.cloud_output += usage.output;
            }
        }
        self.telemetry
            .lock()
            .map_err(|_| Error::Model("telemetry lock".into()))?
            .push(RoutingTelemetry {
                task_kind: req.task_kind.clone(),
                route,
                reason,
                runtime: format!("{route:?}"),
                latency_ms: start.elapsed().as_millis() as u64,
                usage,
                validation_passed: valid,
                escalations,
            });
        Ok(())
    }
}

/// FIFO admission, one active request. Cancelled queue entries release their position.
#[derive(Default)]
pub struct LocalInferenceScheduler {
    state: Mutex<SchedulerState>,
    changed: Condvar,
    next: AtomicU64,
}
#[derive(Default)]
struct SchedulerState {
    active: bool,
    queue: VecDeque<u64>,
}
pub struct InferencePermit<'a> {
    scheduler: &'a LocalInferenceScheduler,
}
impl LocalInferenceScheduler {
    pub fn acquire(&self, cancelled: &AtomicBool) -> Result<InferencePermit<'_>> {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Model("scheduler lock".into()))?;
        state.queue.push_back(ticket);
        loop {
            if cancelled.load(Ordering::Relaxed) {
                state.queue.retain(|t| *t != ticket);
                self.changed.notify_all();
                return Err(Error::Model("inference cancelled".into()));
            }
            if !state.active && state.queue.front() == Some(&ticket) {
                state.queue.pop_front();
                state.active = true;
                return Ok(InferencePermit { scheduler: self });
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .map_err(|_| Error::Model("scheduler wait".into()))?
                .0;
        }
    }
}
impl Drop for InferencePermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.scheduler.state.lock() {
            state.active = false;
            self.scheduler.changed.notify_all();
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerRequest {
    pub work_node_id: WorkNodeId,
    pub context_snapshot_id: ContextSnapshotId,
    pub capabilities: Vec<String>,
    pub budget: WorkBudget,
    pub output_schema: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub version: String,
    pub instruction_version: String,
    pub required_capabilities: Vec<String>,
    pub permitted_tools: Vec<String>,
    pub context_policy: String,
    pub output_schema: Value,
    pub quality_checks: Vec<String>,
}
#[derive(Default)]
pub struct SkillRegistry {
    pub skills: BTreeMap<String, SkillDefinition>,
}
impl SkillRegistry {
    pub fn builtins() -> Self {
        let mut registry = Self::default();
        for name in [
            "literature_search",
            "evidence_extraction",
            "contradiction_search",
            "experiment_preparation",
        ] {
            registry.skills.insert(
                name.into(),
                SkillDefinition {
                    name: name.into(),
                    version: "1".into(),
                    instruction_version: "v2.1".into(),
                    required_capabilities: vec!["structured_output".into()],
                    permitted_tools: vec!["read_file".into()],
                    context_policy: "bounded-evidence-v1".into(),
                    output_schema: object_schema(
                        json!({"summary":{"type":"string"}}),
                        &["summary"],
                    ),
                    quality_checks: vec![
                        "schema".into(),
                        "source grounding".into(),
                        "human gate for scientific changes".into(),
                    ],
                },
            );
        }
        registry
    }
}
