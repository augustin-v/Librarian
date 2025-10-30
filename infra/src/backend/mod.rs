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
use std::fs::File;
use opentelemetry::trace::Status;
use rig::Embed;
use rig::agent::Agent;
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
use x402_rs::{address_evm, address_sol};

// placeholder MCP data for now
#[derive(Embed, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct McpEntry {
    #[embed]
    pub name: String,
    pub endpoint: String,
    pub version: String,
    #[embed]
    pub capabilities: Vec<String>,
    #[embed]
    pub desc: String,
}

pub fn load_mcps_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<McpEntry>> {
    let file = File::open(&path)
        .with_context(|| format!("Failed to open {:?}", path.as_ref()))?;
    let entries: Vec<McpEntry> = serde_json::from_reader(file)
        .with_context(|| "Failed to parse mcps.json into Vec<McpEntry>")?;
    Ok(entries)
}

#[derive(Deserialize)]
pub struct DiscoverRequest {
    pub query: String,
    pub filters: Option<Value>,
    pub client_type: Option<String>,
}

#[tracing::instrument(skip_all)]
async fn discover_handler(
    State(agent): State<Arc<Agent<ResponsesCompletionModel>>>,
    Json(req): Json<DiscoverRequest>,
) -> impl IntoResponse {
    let query = req.query;

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
    pub agent: Arc<Agent<ResponsesCompletionModel>>,
}

impl Backend {
    pub fn new(agent: Agent<ResponsesCompletionModel>) -> Self {
        let facilitator_url = env::var("FACILITATOR_URL")
            .unwrap_or_else(|_| "https://facilitator.x402.rs".to_string());

        let base_url = env::var("API_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8080/".to_string());
        let x402_base = X402Middleware::try_from(facilitator_url.clone())
            .expect("Failed to create X402 middleware")
            .with_base_url(url::Url::parse(&base_url).expect("Invalid base URL"));

        let usdc_base_sepolia = USDCDeployment::by_network(Network::BaseSepolia)
            .pay_to(address_evm!("0xf2757Fe8Ba90ad98dAed8e6254bA9A677069826a"));
        let usdc_solana = USDCDeployment::by_network(Network::Solana)
            .pay_to(address_sol!("11111111111111111111111111111112"));

        let agent_arc = Arc::new(agent);

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
        let facilitator_url =
        env::var("FACILITATOR_URL").unwrap_or_else(|_| "https://facilitator.x402.rs".to_string());
        let base_url = env::var("API_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8080/".to_string());
        let port = env::var("API_PORT").unwrap_or_else(|_| "8080".to_string());
        let bind_addr = format!("0.0.0.0:{}", port);

        let x402_base = X402Middleware::try_from(facilitator_url)
            .expect("Failed to create X402 middleware")
            .with_base_url(url::Url::parse(&base_url).expect("Invalid base URL"));

        tracing::info!("Using facilitator on {}", x402_base.facilitator_url());

        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .with_context(|| format!("Failed to bind to {}", bind_addr))?;
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
