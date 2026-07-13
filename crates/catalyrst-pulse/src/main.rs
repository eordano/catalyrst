use catalyrst_pulse::PulseServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let bind = std::env::var("PULSE_BIND")
        .unwrap_or_else(|_| "0.0.0.0:9000".to_string())
        .parse()?;
    PulseServer::new().run(bind).await
}
