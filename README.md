## kubers-sample

A toy custom controller written by Rust.

### Register CustomResourceDefinition

```bash
$ cargo run --bin crdgen | kubectl apply -f -
```

### Run Custom Controller

```bash
$ cargo run --bin controller
```

### Create Custom Resource

```bash
# Create custom resource
$ kubectl apply -f config/sample/sample.yaml

$ kubectl get jobs.kitagry.github.io
NAME     AGE
sample   1m0s

$ kubectl get pods
NAME            READY   STATUS      RESTARTS   AGE
sample-m7vbtg   1/1     Running     0          1m0s

# Delete custom resource
$ kubectl delete -f config/sample/sample.yaml
job.kitagry.github.io "sample" deleted

# Pod is deleted, too.
$ kubectl get pods
No resources found in default namespace.
```
