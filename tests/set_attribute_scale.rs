//! `Document::set_attribute` must not scan the element's attributes per call.

use moxml::document::*;
use std::time::Instant;

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("a{i}")).collect()
}

/// Same-named sets replace in place and keep order, also past the point
/// where an index is in use; a different namespace is a different attribute.
#[test]
fn replace_keeps_position_with_many_attributes() {
    let mut doc = Document::with_root(QName::local("r")).unwrap();
    let root = doc.root();
    let ns = QName {
        prefix: Some("p".into()),
        local: "a5".into(),
        namespace: Some(std::sync::Arc::from("urn:p")),
    };
    for (i, n) in names(200).iter().enumerate() {
        doc.set_attribute(root, QName::local(n), &i.to_string())
            .unwrap();
    }
    doc.set_attribute(root, QName::local("a5"), "new").unwrap();
    doc.set_attribute(root, QName::local("a199"), "last")
        .unwrap();
    doc.set_attribute(root, ns, "ns").unwrap();
    doc.set_attribute(root, QName::local("a5"), "newer")
        .unwrap();
    let attrs = doc.attributes(root).unwrap();
    assert_eq!(attrs.len(), 201);
    for (i, a) in attrs.iter().take(200).enumerate() {
        assert_eq!(a.name.local, format!("a{i}"));
        let want = match i {
            5 => "newer".to_string(),
            199 => "last".to_string(),
            _ => i.to_string(),
        };
        assert_eq!(a.value, want);
    }
    assert_eq!(attrs[200].value, "ns");
    assert!(attrs[200].name.namespace.is_some());
    // a clone keeps working independently
    let mut copy = doc.clone();
    copy.set_attribute(root, QName::local("a0"), "c").unwrap();
    assert_eq!(doc.attributes(root).unwrap()[0].value, "0");
    assert_eq!(copy.attributes(root).unwrap()[0].value, "c");
}

/// Timing probe: `cargo test --release --test set_attribute_scale -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_set_attribute() {
    for n in [1_000usize, 10_000, 50_000] {
        let ns = names(n);
        let mut doc = Document::with_root(QName::local("r")).unwrap();
        let root = doc.root();
        let t = Instant::now();
        for s in &ns {
            doc.set_attribute(root, QName::local(s), "v").unwrap();
        }
        println!("set_attribute N={n}: {:?}", t.elapsed());
    }
}
