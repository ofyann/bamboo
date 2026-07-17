use bamboo::image::ImageRef;
use std::str::FromStr;

#[test]
fn test_docker_io_library_default_tag() {
    let img = ImageRef::from_str("nginx").unwrap();
    assert_eq!(img.registry, "docker.io");
    assert_eq!(img.namespace, "");
    assert_eq!(img.name, "nginx");
    assert_eq!(img.tag, "latest");

    let normalized = img.normalize();
    assert_eq!(normalized.namespace, "library");
    assert_eq!(normalized.hubproxy_path(), "library/nginx");
    assert_eq!(normalized.dest_path(), "library/nginx");
}

#[test]
fn test_docker_io_with_namespace() {
    let img = ImageRef::from_str("example/app:v1.2").unwrap();
    assert_eq!(img.registry, "docker.io");
    assert_eq!(img.namespace, "example");
    assert_eq!(img.name, "app");
    assert_eq!(img.tag, "v1.2");
    assert_eq!(img.normalize().hubproxy_path(), "example/app");
}

#[test]
fn test_quay_io() {
    let img = ImageRef::from_str("quay.io/coreos/etcd:v3.5").unwrap();
    assert_eq!(img.registry, "quay.io");
    assert_eq!(img.namespace, "coreos");
    assert_eq!(img.name, "etcd");
    assert_eq!(img.tag, "v3.5");
    assert_eq!(img.hubproxy_path(), "quay.io/coreos/etcd");
}

#[test]
fn test_localhost_registry() {
    let img = ImageRef::from_str("localhost:5000/myapp:latest").unwrap();
    assert_eq!(img.registry, "localhost:5000");
    assert_eq!(img.namespace, "");
    assert_eq!(img.name, "myapp");
    assert_eq!(img.tag, "latest");
}

#[test]
fn test_port_in_tag_not_confused() {
    // "nginx:1.25" should not be treated as registry:port
    let img = ImageRef::from_str("nginx:1.25").unwrap();
    assert_eq!(img.registry, "docker.io");
    assert_eq!(img.name, "nginx");
    assert_eq!(img.tag, "1.25");
}
