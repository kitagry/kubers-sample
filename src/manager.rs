use crate::Error;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::api::{DeleteParams, ListParams, PostParams};
use kube::{Api, Client, CustomResource, Resource};
use kube_runtime::{
    controller::{Context, ReconcilerAction},
    Controller,
};
use prometheus::{default_registry, proto::MetricFamily};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

async fn reconcile(job: Job, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    println!("{:?}", job);
    let pods = Api::<Pod>::namespaced(
        ctx.get_ref().client.clone(),
        &job.metadata.namespace.as_ref().unwrap(),
    );

    let lp = ListParams::default();
    match pods.list(&lp).await {
        Ok(pod_list) => {
            let job_uid = job.metadata.uid.as_ref().unwrap();
            let owned_pods = pod_list.iter().filter(|pod| {
                pod.metadata
                    .owner_references
                    .as_ref()
                    .unwrap_or(&vec![])
                    .iter()
                    .any(|or| &or.uid == job_uid)
            });
            for pod in owned_pods {
                let container = &pod.spec.as_ref().unwrap().containers[0];
                if container.name == job.spec.name
                    && container.image.as_ref().unwrap() == &job.spec.image
                {
                    return Ok(ReconcilerAction {
                        requeue_after: None,
                    });
                }
                let dp = DeleteParams::default();
                pods.delete(&pod.metadata.name.as_ref().unwrap(), &dp)
                    .await
                    .or_else(|e| Err(Error::KubeError(e)))?;
            }
            create_pod(pods, job).await?;
        }
        Err(kube::Error::Api(e)) if e.code == 404 => {
            create_pod(pods, job).await?;
        }
        Err(e) => return Err(Error::KubeError(e)),
    };
    Ok(ReconcilerAction {
        requeue_after: None,
    })
}

fn object_to_owner_reference<K: Resource<DynamicType = ()>>(
    meta: ObjectMeta,
) -> Result<OwnerReference, Error> {
    Ok(OwnerReference {
        api_version: K::api_version(&()).to_string(),
        kind: K::kind(&()).to_string(),
        name: meta.name.ok_or(Error::MissingObjectKey {
            name: ".metadata.name",
        })?,
        uid: meta.uid.ok_or(Error::MissingObjectKey {
            name: ".metadata.uid",
        })?,
        ..OwnerReference::default()
    })
}

async fn create_pod(pods: Api<Pod>, job: Job) -> Result<(), Error> {
    let pod = Pod {
        metadata: ObjectMeta {
            namespace: job.metadata.namespace.clone(),
            name: Some(create_tmp_pod_name(job.metadata.name.as_ref().unwrap())),
            owner_references: Some(vec![object_to_owner_reference::<Job>(job.metadata)?]),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: job.spec.name,
                image: Some(job.spec.image),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: None,
    };
    let pp = PostParams::default();

    match pods.create(&pp, &pod).await {
        Ok(_) => Ok(()),
        Err(e) => Err(Error::KubeError(e)),
    }
}

fn create_tmp_pod_name(s: &str) -> String {
    let rand_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();
    let rand_string = rand_string.to_lowercase();
    match s.len() {
        _ => format!("{}-{}", s, rand_string),
    }
}

fn error_policy(error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    println!("reconcile failed: {:?}", error);
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(360)),
    }
}

/// Data owned by the Manager
#[derive(Clone)]
pub struct Manager {
    // Various prometheus metrics
// metrics: Metrics,
}

impl Manager {
    pub async fn new() -> (Self, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let context = Context::new(Data {
            client: client.clone(),
        });

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

        (Self {}, drainer)
    }

    pub fn metrics(&self) -> Vec<MetricFamily> {
        default_registry().gather()
    }
}
