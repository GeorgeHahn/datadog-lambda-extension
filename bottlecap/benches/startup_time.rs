use std::sync::Arc;

use axum::{
    extract::State,
    response::Response,
    routing::{get, post},
    Router,
};
use bottlecap::EXTENSION_ID_HEADER;
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use tempfile::tempdir;

#[path = "../src/bin/bottlecap/main.rs"]
mod bottlecap_bin;

fn criterion_benchmark(c: &mut Criterion) {
    // config directory (optionally contains datadog.yaml)
    let tmpdir = tempdir().unwrap();

    // config envs
    std::env::set_var("DD_API_KEY", "dd-api-key");
    std::env::set_var("DD_SITE", "127.0.0.1:3001"); // (nothing here)

    // mock envs from control plane
    std::env::set_var("AWS_DEFAULT_REGION", "us-east-1");
    std::env::set_var("AWS_ACCESS_KEY_ID", "aws-access-key-id");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret-access-key");
    std::env::set_var("AWS_SESSION_TOKEN", "session-token");
    std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", "lambda-function-name");
    std::env::set_var("AWS_LAMBDA_RUNTIME_API", "127.0.0.1:3000");
    std::env::set_var("LAMBDA_TASK_ROOT", tmpdir.path().display().to_string());

    // Separate runtime for the mock control plane.
    let mock_rt = tokio::runtime::Runtime::new().unwrap();

    // setup runs for each iteration - listen for http connections & notify when /next is hit
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    mock_rt.spawn(async { mock_control_plane(tx).await });

    c.bench_function("time-to-init", |b| {
        b.iter_batched(
            || {},
            |_| {
                // run bottlecap on a fresh tokio runtime - the idea is to
                // capture any tokio runtime overhead in the benchmark & ensure
                // it's fully cleaned up between iterations. Sort of hacky to
                // not call the synchronous main fn; thing is, you can't
                // portably kill a thread, so this is a whole lot easier than
                // trying to run bottlecap's main().
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async {
                    tokio::select! {
                        // run bottlecap
                        _ = bottlecap_bin::inner_main() => {},
                        // until the `/next` endpoint is hit
                        _ = rx.recv() => {},
                    }
                });

                // cleanup. This needs to be cheap because it's included in the
                // benchmark. (Consider using a custom criterion timing loop.)
                rt.shutdown_background();
                // mock_task.abort();
            },
            // Requests must not run in parallel due to the single channel
            // signaling method used for test completion.
            criterion::BatchSize::PerIteration,
        );
    });
}

#[derive(Clone)]
struct AppState {
    tx: Arc<tokio::sync::mpsc::Sender<()>>,
}

async fn mock_control_plane(tx: tokio::sync::mpsc::Sender<()>) {
    let state = AppState { tx: Arc::new(tx) };

    // build our application with a route
    let app = Router::new()
        .route("/2020-01-01/extension/register", post(register_mock))
        .route("/2020-01-01/extension/event/next", get(next_mock))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn next_mock(State(state): State<AppState>) -> Response {
    state.tx.send(()).await.unwrap();

    Response::builder()
        .status(400)
        .body(axum::body::Body::empty())
        .unwrap()
}

async fn register_mock(State(_state): State<AppState>) -> Response {
    Response::builder()
        .status(200)
        .header(EXTENSION_ID_HEADER, "mock-extension-id")
        .body(
            json!(
                {
                    "account_id": "mock-account-id",
                }
            )
            .to_string()
            .into(),
        )
        .unwrap()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
