use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use dotenvy::dotenv;
use reqwest::Client;
use serde_json::json;
use std::env;
use x402_reqwest::{MaxTokenAmountFromAmount, ReqwestWithPayments, ReqwestWithPaymentsBuild};
use x402_rs::network::{Network, USDCDeployment};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let request_body = json!({
        "query": "I need to build a cool frontend",
        "filters": {"latency": "low", "cost": "<0.001"},
        "client_type": "native"
    });

    let signer: PrivateKeySigner = env::var("EVM_PRIVATE_KEY")?.parse().context("Invalid EVM private key")?;
    let sender = x402_reqwest::chains::evm::EvmSenderWallet::new(signer);

    let http_client = Client::new()
        .with_payments(sender)
        .prefer(USDCDeployment::by_network(Network::BaseSepolia))
        .max(USDCDeployment::by_network(Network::BaseSepolia).amount(0.1)?)
        .build();

    let body_str = serde_json::to_string(&request_body).context("Failed to serialize JSON body")?;

    let response = http_client
        .post("http://localhost:8080/discover")
        .header("Content-Type", "application/json")
        .body(body_str)
        .send()
        .await?;

    let status = response.status();
    let headers = response.headers().clone();
    let text = response.text().await?;
    println!("Status: {:?}\nHeaders: {:?}\nResponse: {}", status, headers, text);

    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        println!("402: Check wallet balance (0.001+ USDC on Base Sepolia), key validity, or tx on basescan.org (search pay_to addr).");
    }

    Ok(())
}