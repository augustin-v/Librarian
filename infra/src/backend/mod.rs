// src/backend/mod.rs
use crate::ResponsesCompletionModel;
use anyhow::{Context as _, Result};
use axum::{
    Router,
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Json as AxumJson},
    routing::{get, post},
};
use opentelemetry::trace::Status;
use rig::Embed;
use rig::agent::Agent;
use rig::completion::CompletionModel;
use rig::completion::Prompt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use x402_axum::{IntoPriceTag, X402Middleware};
use x402_rs::network::{Network, USDCDeployment};
use x402_rs::{address_evm, address_sol}; // keep if other places need this

// Placeholder MCP data (simple struct for RAG; derives Embed on desc for now)
#[derive(Embed, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct McpEntry {
    pub name: String,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<String>,
    #[embed]
    pub desc: String,
}

pub fn sample_mcps() -> Vec<McpEntry> {
    /* same as before */
    vec![
        McpEntry {
            name: "UnityForge".to_string(),
            endpoint: "https://unityforge.example.com/mcp/v1".to_string(),
            version: "1.2".to_string(),
            capabilities: vec!["build_scene".to_string(), "import_asset".to_string(), "run_physics".to_string()],
            desc: "UnityForge: Game engine MCP for AI agents. Supports C# scripting, asset loading, physics simulations, and scene building with low latency.".to_string(),
        },
        McpEntry {
            name: "FinanceAPI".to_string(),
            endpoint: "https://financeapi.example.com/mcp/v1".to_string(),
            version: "1.3".to_string(),
            capabilities: vec!["fetch_stocks".to_string(), "analyze_trends".to_string()],
            desc: "FinanceAPI: Financial data MCP for trading agents. Provides stock fetches, market trends, portfolio analysis, and risk modeling.".to_string(),
        },
    ]
}

#[derive(Deserialize)]
pub struct DiscoverRequest {
    pub query: String,
    pub filters: Option<Value>,
    pub client_type: Option<String>,
}

// NOTE: handler now accepts an Arc<Agent<ResponsesCompletionModel>> as state
#[tracing::instrument(skip_all)]
async fn discover_handler(
    State(agent): State<Arc<Agent<ResponsesCompletionModel>>>,
    Json(req): Json<DiscoverRequest>,
) -> impl IntoResponse {
    let query = req.query;

    // Simplified prompt as string — keep same Prompt usage if rig requires it:
    let prompt = format!(
        "User query: {}. As Librarian, recommend a tool match and explain briefly.",
        query
    );

    match agent.as_ref().prompt(&prompt).await {
        Ok(response) => {
            let json_resp = Value::String(format!(
                "Discovered via RAG: {} (Agent response: {})",
                query, response
            ));
            (StatusCode::OK, AxumJson(json_resp))
        }
        Err(e) => {
            let json_resp = Value::String(format!("Agent error: {}", e));
            (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json_resp))
        }
    }
}

pub struct Backend {
    pub app: Router,
    pub agent: Arc<Agent<ResponsesCompletionModel>>, // store Arc so we can clone into router state
}

impl Backend {
    pub fn new(agent: Agent<ResponsesCompletionModel>) -> Self {
        let facilitator_url = env::var("FACILITATOR_URL")
            .unwrap_or_else(|_| "https://facilitator.x402.rs".to_string());

        let x402_base = X402Middleware::try_from(facilitator_url.clone())
            .expect("Failed to create X402 middleware")
            .with_base_url(url::Url::parse("http://localhost:3000/").expect("Invalid base URL"));

        let usdc_base_sepolia = USDCDeployment::by_network(Network::BaseSepolia)
            .pay_to(address_evm!("0xf2757Fe8Ba90ad98dAed8e6254bA9A677069826a"));
        let usdc_solana = USDCDeployment::by_network(Network::Solana)
            .pay_to(address_sol!("11111111111111111111111111111112"));

        // wrap agent into Arc so we can put it into router state
        let agent_arc = Arc::new(agent);

        // Build router and attach state that is just the Arc<Agent<_>>
        let app = Router::new()
            .route("/health", get(|| async { "OK" }))
            .route(
                "/discover",
                post(discover_handler).layer(
                    x402_base
                        .clone()
                        .with_description("MCP Discovery Service")
                        .with_mime_type("application/json")
                        .with_price_tag(usdc_solana.amount(0.001).unwrap())
                        .or_price_tag(usdc_base_sepolia.amount(0.001).unwrap()),
                ),
            )
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|request: &axum::http::Request<_>| {
                        info_span!(
                            "http_request",
                            otel.kind = "server",
                            otel.name = %format!("{} {}", request.method(), request.uri()),
                            method = %request.method(),
                            uri = %request.uri(),
                            version = ?request.version(),
                        )
                    })
                    .on_response(
                        |response: &axum::http::Response<_>,
                         latency: std::time::Duration,
                         span: &tracing::Span| {
                            span.record("status", tracing::field::display(response.status()));
                            span.record("latency", tracing::field::display(latency.as_millis()));
                            span.record(
                                "http.status_code",
                                tracing::field::display(response.status().as_u16()),
                            );

                            if response.status().is_success()
                                || response.status() == StatusCode::PAYMENT_REQUIRED
                            {
                                span.set_status(Status::Ok);
                            } else {
                                span.set_status(Status::error(
                                    response
                                        .status()
                                        .canonical_reason()
                                        .unwrap_or("unknown")
                                        .to_string(),
                                ));
                            }

                            tracing::info!(
                                "status={} elapsed={}ms",
                                response.status().as_u16(),
                                latency.as_millis()
                            );
                        },
                    ),
            )
            // attach only the agent as shared state
            .with_state(Arc::clone(&agent_arc));

        Backend {
            app,
            agent: agent_arc,
        }
    }

    pub async fn launch(self) -> Result<()> {
        // Test the agent via arc reference
        let test_prompt = "Test launch: Confirm Librarian ready.";
        match self.agent.as_ref().prompt(test_prompt).await {
            Ok(resp) => tracing::info!("Agent launched successfully: {}", resp),
            Err(e) => tracing::warn!("Agent launch test failed: {}", e),
        }

        let facilitator_url = env::var("FACILITATOR_URL")
            .unwrap_or_else(|_| "https://facilitator.x402.rs".to_string());

        let x402_base = X402Middleware::try_from(facilitator_url)
            .expect("Failed to create X402 middleware")
            .with_base_url(url::Url::parse("http://localhost:3000/").expect("Invalid base URL"));

        tracing::info!("Using facilitator on {}", x402_base.facilitator_url());

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
            .await
            .context("Failed to bind to 0.0.0.0:3000")?;
        tracing::info!("Listening on {}", listener.local_addr().unwrap());

        // Serve the router that already has state attached
        axum::serve(listener, self.app)
            .into_future()
            .instrument(info_span!("axum_server"))
            .await
            .context("Server failed to run")?;
        Ok(())
    }
}
