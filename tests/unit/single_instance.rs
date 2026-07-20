// @author kongweiguang

use super::{
    InstanceLaunch, InstanceMessage, MAX_PATHS, NACK, PROTOCOL_MAGIC, acquire_with_paths,
    instance_socket_path, read_message, write_message,
};
use futures::StreamExt as _;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use uds_windows::UnixStream;

#[test]
fn protocol_round_trips_unicode_paths_and_activate_message() {
    for paths in [
        vec![PathBuf::from(r"C:\notes\中文.md")],
        Vec::<PathBuf>::new(),
    ] {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &paths).unwrap();
        assert_eq!(
            read_message(bytes.as_slice()).unwrap(),
            InstanceMessage { paths }
        );
    }
}

#[test]
fn ui_check_socket_path_isolated_without_changing_production_path() {
    let installation_id = uuid::Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
    let production = instance_socket_path(installation_id, None);
    let first = instance_socket_path(installation_id, Some(r"C:\temp\gmark-ui-a".into()));
    let second = instance_socket_path(installation_id, Some(r"C:\temp\gmark-ui-b".into()));

    assert!(
        production
            .file_name()
            .is_some_and(|name| name == "gmi-6ba7b8109dad11d180b400c04fd430c8.sock")
    );
    assert_ne!(first, production);
    assert_ne!(first, second);
    assert!(first.file_name().is_some_and(|name| name.len() < 80));
}

#[test]
fn protocol_rejects_bad_magic_truncation_and_excessive_count() {
    assert!(read_message(&b"bad"[..]).is_err());
    let mut bytes = PROTOCOL_MAGIC.to_vec();
    bytes.extend_from_slice(&((MAX_PATHS + 1) as u32).to_le_bytes());
    assert!(read_message(bytes.as_slice()).is_err());

    let mut truncated = Vec::new();
    write_message(&mut truncated, &[PathBuf::from("a.md")]).unwrap();
    truncated.pop();
    assert!(read_message(truncated.as_slice()).is_err());
}

#[test]
fn secondary_forwards_to_primary_and_guard_cleans_socket() {
    let root = std::env::temp_dir().join(format!("gmark-instance-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let lock = root.join("instance.lock");
    let socket = std::env::temp_dir().join(format!("gmi-{}.sock", uuid::Uuid::new_v4().simple()));
    let InstanceLaunch::Primary {
        guard,
        mut receiver,
    } = acquire_with_paths(&lock, &socket, &[]).unwrap()
    else {
        panic!("first acquisition must own the instance");
    };
    let paths = vec![PathBuf::from(r"C:\notes\forwarded.md")];
    assert!(matches!(
        acquire_with_paths(&lock, &socket, &paths).unwrap(),
        InstanceLaunch::Forwarded
    ));
    assert_eq!(
        futures::executor::block_on(receiver.next()).unwrap(),
        InstanceMessage {
            paths: paths.clone()
        }
    );
    drop(guard);
    assert!(!socket.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn malformed_client_does_not_poison_following_forward() {
    let root =
        std::env::temp_dir().join(format!("gmark-instance-malformed-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let lock = root.join("instance.lock");
    let socket = std::env::temp_dir().join(format!("gmi-{}.sock", uuid::Uuid::new_v4().simple()));
    let InstanceLaunch::Primary {
        guard,
        mut receiver,
    } = acquire_with_paths(&lock, &socket, &[]).unwrap()
    else {
        panic!("first acquisition must own the instance");
    };

    let mut malformed = UnixStream::connect(&socket).unwrap();
    malformed.write_all(b"not-gmark").unwrap();
    let mut response = [0u8; 1];
    malformed.read_exact(&mut response).unwrap();
    assert_eq!(response, [NACK]);

    let paths = vec![PathBuf::from(r"C:\notes\after-malformed.md")];
    assert!(matches!(
        acquire_with_paths(&lock, &socket, &paths).unwrap(),
        InstanceLaunch::Forwarded
    ));
    assert_eq!(
        futures::executor::block_on(receiver.next()).unwrap().paths,
        paths
    );
    drop(guard);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn racing_starts_elect_exactly_one_primary() {
    let root = std::env::temp_dir().join(format!("gmark-instance-race-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let lock = root.join("instance.lock");
    let socket = std::env::temp_dir().join(format!("gmi-{}.sock", uuid::Uuid::new_v4().simple()));
    let barrier = Arc::new(Barrier::new(3));
    let threads = ["first.md", "second.md"].map(|name| {
        let lock = lock.clone();
        let socket = socket.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            acquire_with_paths(&lock, &socket, &[PathBuf::from(name)]).unwrap()
        })
    });
    barrier.wait();
    let [first, second] = threads.map(|thread| thread.join().unwrap());
    let mut primary = None;
    let mut forwarded = 0;
    for launch in [first, second] {
        match launch {
            InstanceLaunch::Primary { guard, receiver } => {
                assert!(primary.replace((guard, receiver)).is_none());
            }
            InstanceLaunch::Forwarded => forwarded += 1,
        }
    }
    assert_eq!(forwarded, 1);
    let (guard, mut receiver) = primary.expect("one start must own the instance");
    let message = futures::executor::block_on(receiver.next()).unwrap();
    assert_eq!(message.paths.len(), 1);
    assert!(matches!(
        message.paths[0].to_string_lossy().as_ref(),
        "first.md" | "second.md"
    ));
    drop(guard);
    let _ = std::fs::remove_dir_all(root);
}
