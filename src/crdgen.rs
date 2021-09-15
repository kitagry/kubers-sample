use kube::CustomResourceExt;
fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&kubers_sample::Job::crd()).unwrap()
    )
}
