//! Host tests of the delivered representation source, not Rill execution.
use rill_error_storage::{Error, OwnedErr, SourceLocation};
use std::sync::{Arc, Mutex};

#[test]
fn formats_owned_heterogeneous_causes_with_original_sites() {
    struct Number(u32);
    let source_name = String::from("user.rill");
    let inner = OwnedErr::new(Number(17), SourceLocation::new(&source_name, 12));
    let outer = OwnedErr::wrapping(
        Error::new("context".into()),
        SourceLocation::new("entry.rill", 3),
        inner,
        |number| format!("number {}", number.0),
    );
    drop(source_name);
    assert_eq!(
        outer.format(Error::display),
        "context (entry.rill:byte 3)\ncaused by: number 17 (user.rill:byte 12)"
    );
    let mapped = outer.map(|value| value.display().len());
    assert_eq!(mapped.value(), &7);
    assert_eq!(
        mapped.format(usize::to_string),
        "7 (entry.rill:byte 3)\ncaused by: number 17 (user.rill:byte 12)"
    );
    assert_eq!(mapped.into_value(), 7);
}

#[test]
fn deep_chain_releases_each_owned_payload_without_recursive_drop() {
    struct Payload(usize, Arc<Mutex<Vec<usize>>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.1.lock().unwrap().push(self.0);
        }
    }
    let released = Arc::new(Mutex::new(Vec::new()));
    let worker = {
        let released = released.clone();
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                let mut current = OwnedErr::new(
                    Payload(0, released.clone()),
                    SourceLocation::new("src.rill", 0),
                );
                for i in 1..10_000 {
                    current = OwnedErr::wrapping(
                        Payload(i, released.clone()),
                        SourceLocation::new("src.rill", i as u64),
                        current,
                        |p| p.0.to_string(),
                    );
                }
                let rendered = current.format(|p| p.0.to_string());
                assert!(rendered.starts_with("9999 (src.rill:byte 9999)"));
                assert!(rendered.ends_with("caused by: 0 (src.rill:byte 0)"));
                assert_eq!(rendered.lines().count(), 10_000);
                assert!(released.lock().unwrap().is_empty());
                drop(current);
            })
            .unwrap()
    };
    worker.join().unwrap();
    assert!(
        released
            .lock()
            .unwrap()
            .iter()
            .copied()
            .eq((0..10_000).rev())
    );
}
