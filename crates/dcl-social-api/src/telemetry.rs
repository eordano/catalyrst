use super::*;
use std::sync::OnceLock;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientError {
    kind: String,
    message: String,
    #[serde(default)]
    stack: String,
}
enum Item {
    Event(Value),
    Flush(oneshot::Sender<()>),
}
static QUEUE: OnceLock<mpsc::Sender<Item>> = OnceLock::new();
fn scrub(text: &str) -> String {
    static PRIVATE: OnceLock<regex::Regex> = OnceLock::new();
    let regex = PRIVATE.get_or_init(|| regex::Regex::new(r"(?i)0x[a-f0-9]{40,}|(?:bearer\s+)[^\s]+|(?:privatekey|signature|token|password)[\s\x22:=]+[^\s,}]+|\?[^\s)]+").unwrap());
    regex
        .replace_all(text, "[redacted]")
        .chars()
        .take(4000)
        .collect()
}
fn release() -> &'static str {
    static RELEASE: OnceLock<String> = OnceLock::new();
    RELEASE.get_or_init(|| {
        std::env::var("SOCIAL_RELEASE")
            .ok()
            .or_else(|| {
                // Production runs releases/<release>/bin/dcl-social-api. The OS
                // resolves the current symlink for us, identifying this binary.
                let exe = std::env::current_exe().ok()?;
                let name = exe.parent()?.parent()?.file_name()?.to_str()?;
                name.starts_with("dcl-social-").then(|| name.to_owned())
            })
            .unwrap_or_else(|| format!("dcl-social-{}", env!("CARGO_PKG_VERSION")))
    })
}
pub fn emit(source: &str, kind: &str, message: &str, stack: &str) {
    let Some(queue) = QUEUE.get() else {
        return;
    };
    let message = scrub(message);
    let event = json!({"event_id":Uuid::new_v4().simple().to_string(),"timestamp":chrono::Utc::now().to_rfc3339(),"level":"error","platform":if source=="client" {"javascript"} else {"rust"},"environment":"production","release":release(),"message":message,"exception":{"values":[{"type":scrub(kind),"value":message}]},"tags":{"app":"dcl.social","component":source},"extra":{"stack":scrub(stack)}});
    let _ = queue.try_send(Item::Event(event));
}
pub fn init() {
    let (tx, mut rx) = mpsc::channel(128);
    if QUEUE.set(tx).is_err() {
        return;
    }
    let url = std::env::var("SOCIAL_TELEMETRY_URL")
        .unwrap_or_else(|_| "https://interconnected.online/telemetry/api/dcl-social/store/".into());
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut recent = std::collections::HashMap::<String, std::time::Instant>::new();
        while let Some(item) = rx.recv().await {
            match item {
                Item::Flush(done) => {
                    let _ = done.send(());
                }
                Item::Event(event) => {
                    recent.retain(|_, time| time.elapsed() < Duration::from_secs(60));
                    let key = format!("{}:{}", event["tags"]["component"], event["message"]);
                    if recent.contains_key(&key) || recent.len() >= 30 {
                        continue;
                    }
                    recent.insert(key, std::time::Instant::now());
                    // Reporting is bounded and never recursively reports its own failure.
                    match client.post(&url).json(&event).send().await {
                        Ok(r) if r.status().is_success() => {}
                        _ => eprintln!("dcl.social telemetry delivery failed"),
                    }
                }
            }
        }
    });
    struct Errors;
    impl<S: tracing::Subscriber> Layer<S> for Errors {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() != tracing::Level::ERROR {
                return;
            }
            #[derive(Default)]
            struct Message(String);
            impl tracing::field::Visit for Message {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut message = Message::default();
            event.record(&mut message);
            emit("server", event.metadata().target(), &message.0, "");
        }
    }
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .with(Errors)
        .try_init();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        emit("server", "panic", &info.to_string(), "");
        previous(info);
    }));
}
pub async fn flush() {
    if let Some(queue) = QUEUE.get() {
        let (tx, rx) = oneshot::channel();
        if queue.try_send(Item::Flush(tx)).is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(4), rx).await;
        }
    }
}
pub async fn client_error(Json(error): Json<ClientError>) -> ApiResult<StatusCode> {
    if error.kind.len() > 100 || error.message.len() > 8000 || error.stack.len() > 16000 {
        return Err(bad("Error report too large"));
    }
    emit("client", &error.kind, &error.message, &error.stack);
    Ok(StatusCode::ACCEPTED)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_secrets_from_errors() {
        let value = scrub("signature=secret token=abc bearer credential https://dcl.social/api?private=secret 0x0123456789012345678901234567890123456789");
        for secret in ["secret", "abc", "credential", "0123456789"] {
            assert!(!value.contains(secret));
        }
    }
}
