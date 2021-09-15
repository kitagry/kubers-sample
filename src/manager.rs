use crate::Error;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{api::ListParams, Api, Client, CustomResource};
use kube_runtime::{
    controller::{Context, ReconcilerAction},
    Controller,
};
use prometheus::{default_registry, proto::MetricFamily};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "Job",
    group = "kitagry.github.io",
    version = "v1beta1",
    namespaced
)]
// #[kube(status = "FooStatus")]
pub struct JobSpec {
    name: String,
    image: String,
}

#[derive(Clone)]
struct Data {
    client: Client,
}

async fn reconcile(_job: Job, _ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    Ok(ReconcilerAction {
        requeue_after: None,
    })
}

fn error_policy(_errorr: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(360)),
    }
}

#[derive(Clone, Serialize)]
pub struct State {}

impl State {
    fn new() -> Self {
        Self {}
    }
}

/// Data owned by the Manager
#[derive(Clone)]
pub struct Manager {
    /// In memory state
    state: Arc<RwLock<State>>,
    // Various prometheus metrics
    // metrics: Metrics,
}

impl Manager {
    pub async fn new() -> (Self, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let context = Context::new(Data {
            client: client.clone(),
        });
        let state = Arc::new(RwLock::new(State::new()));

        let jobs = Api::<Job>::all(client);
        let _r = jobs.list(&ListParams::default().limit(1)).await.expect(
            "is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -",
        );

        // All good. Start controller and return its future.
        let drainer = Controller::new(jobs, ListParams::default())
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        (Self { state }, drainer)
    }

    pub fn metrics(&self) -> Vec<MetricFamily> {
        default_registry().gather()
    }

    pub async fn state(&self) -> State {
        self.state.read().await.clone()
    }
}
