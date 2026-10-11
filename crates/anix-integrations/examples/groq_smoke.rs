//! One synthetic compact decision through the production adapter, not research acceptance.
use anix_agent::{
    AgentDecision, ModelProvider, ModelRequest, ModelSelection, ModelTask, NamedToolResult,
    ProcessAction,
};
use anix_core::{ResearchContext, ToolOutput};
use anix_domain::*;
use anix_integrations::{OpenAiCompatibleModelProvider, TestLeadConfig};
use serde_json::json;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = TestLeadConfig::read(std::path::Path::new("config/groq-research-lead.toml"))?;
    if !config.is_groq() {
        return Err("smoke requires the explicit Groq profile".into());
    }
    let provider = OpenAiCompatibleModelProvider::new(config)?;
    let project = ProjectId::new();
    let work = WorkNodeId::new();
    let request = ModelRequest {
        task: ModelTask {
            goal: "Summarize the supplied synthetic measurement, preserving its units.".into(),
            focus: None,
            current_work: Some(WorkNode {
                id: work,
                meta: Meta::new(project),
                branch_id: BranchId::new(),
                kind: "synthetic-smoke".into(),
                granularity: WorkGranularity::Atomic,
                title: "Inspect synthetic observation".into(),
                objective: "Summarize a measured sample of 7 units".into(),
                acceptance_criteria: OutputContract {
                    required_entities: vec![],
                    required_artifacts: vec![],
                    minimum_children: 0,
                },
                status: WorkStatus::Running,
                owner: Actor::LeadAgent("smoke".into()),
                capability_requirements: vec![],
                context_policy_id: "smoke".into(),
                budget: WorkBudget::default(),
                created_by: Actor::LeadAgent("smoke".into()),
                outcome: None,
            }),
            required_tool_succeeded: true,
            retrieval_pending: false,
            retrieval_source_handles: vec![],
        },
        context: ResearchContext {
            project: Project {
                id: project,
                meta: Meta::new(project),
                name: "Synthetic Groq smoke".into(),
                description: "No user research".into(),
                status: ProjectStatus::Active,
            },
            focus: None,
            questions: vec![],
            hypotheses: vec![],
            claims: vec![],
            findings: vec![],
            experiments: vec![],
            neighborhood: vec![],
            pending_gates: vec![],
            recent_events: vec![],
        },
        compiled_context: None,
        selection: ModelSelection {
            provider: "groq".into(),
            model: "openai/gpt-oss-120b".into(),
        },
        output_schema_version: 1,
        max_output_tokens: 1024,
        tools: vec![],
        available_child_tools: vec![],
        allowed_process_actions: None,
        tool_results: vec![NamedToolResult {
            name: "inspect_data".into(),
            output: ToolOutput {
                schema_version: 1,
                data: json!({"sample":"synthetic A","measured_value":7,"unit":"units"}),
                metadata: json!({}),
            },
        }],
        research_branches: vec![],
    };
    let response = futures::executor::block_on(provider.generate(request))?;
    let valid = matches!(&response.decision, AgentDecision::Process {action:ProcessAction::CompleteWorkNode{work_node_id,summary:Some(summary)},..} if *work_node_id==work && summary.contains('7'));
    let report = json!({"provider":"groq","model":"openai/gpt-oss-120b","structured_decision_valid":valid,"response":response,"http_metrics":provider.http_metrics()});
    std::fs::write(
        ".runtime/anix-groq-smoke.json",
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "structured_decision_valid={valid} http_metrics={}",
        provider.http_metrics()
    );
    if !valid {
        return Err("smoke decision did not preserve the supplied measurement".into());
    }
    Ok(())
}
