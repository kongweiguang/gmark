// @author kongweiguang

use super::{
    InstanceLaunch, InstanceMessage, MAX_COMPLETED_REQUEST_IDS, MAX_PATHS, NACK, PROTOCOL_MAGIC,
    acquire_with_paths, instance_socket_path, read_message, remember_completed_request,
    write_message,
};
use futures::StreamExt as _;
use std::collections::{HashSet, VecDeque};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::Duration;
use uds_windows::UnixStream;

#[test]
fn protocol_round_trips_unicode_paths_and_activate_message() {
    for paths in [
        vec![PathBuf::from(r"C:\notes\中文.md")],
        Vec::<PathBuf>::new(),
    ] {
        let mut bytes = Vec::new();
        let message = InstanceMessage {
            request_id: 7,
            paths,
        };
        write_message(&mut bytes, &message).unwrap();
        assert_eq!(read_message(bytes.as_slice()).unwrap(), message);
    }
}

#[test]
fn installation_ids_isolate_socket_paths_without_an_environment_override() {
    let installation_id = uuid::Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
    let second_installation_id =
        uuid::Uuid::parse_str("6ba7b811-9dad-11d1-80b4-00c04fd430c8").unwrap();
    let runtime_root = Path::new(r"C:\Users\gmark\.gmark\runtime");
    let production = instance_socket_path(runtime_root, installation_id).unwrap();
    let first = instance_socket_path(runtime_root, second_installation_id).unwrap();

    assert!(production.starts_with(runtime_root));
    assert_ne!(first, production);
    assert!(first.file_name().is_some_and(|name| name.len() < 80));
}

#[test]
fn overlong_runtime_root_returns_actionable_error() {
    let installation_id = uuid::Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
    let runtime_root = PathBuf::from(r"C:\Users\gmark\.gmark\runtime").join("x".repeat(160));

    assert!(
        runtime_root.to_string_lossy().len() >= 108,
        "the injected runtime root must exceed the Windows AF_UNIX SUN_LEN budget"
    );

    let error = instance_socket_path(&runtime_root, installation_id).unwrap_err();
    let message = format!("{error:#}");

    assert!(message.contains("SUN_LEN"), "{message}");
    assert!(message.contains("runtime root"), "{message}");
}

#[test]
fn protocol_rejects_bad_magic_truncation_and_excessive_count() {
    assert!(read_message(&b"bad"[..]).is_err());
    let mut bytes = PROTOCOL_MAGIC.to_vec();
    bytes.extend_from_slice(&((MAX_PATHS + 1) as u32).to_le_bytes());
    assert!(read_message(bytes.as_slice()).is_err());

    let mut truncated = Vec::new();
    write_message(
        &mut truncated,
        &InstanceMessage {
            request_id: 1,
            paths: vec![PathBuf::from("a.md")],
        },
    )
    .unwrap();
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
    let forwarded = std::thread::spawn({
        let lock = lock.clone();
        let socket = socket.clone();
        let paths = paths.clone();
        move || acquire_with_paths(&lock, &socket, &paths).unwrap()
    });
    let request = futures::executor::block_on(receiver.next()).unwrap();
    assert_eq!(request.message.paths, paths);
    assert!(
        !forwarded.is_finished(),
        "IPC must wait for UI acceptance before ACK"
    );
    request.respond(true);
    assert!(matches!(
        forwarded.join().unwrap(),
        InstanceLaunch::Forwarded
    ));
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
    let forwarded = std::thread::spawn({
        let lock = lock.clone();
        let socket = socket.clone();
        let paths = paths.clone();
        move || acquire_with_paths(&lock, &socket, &paths).unwrap()
    });
    let request = futures::executor::block_on(receiver.next()).unwrap();
    assert_eq!(request.message.paths, paths);
    request.respond(true);
    assert!(matches!(
        forwarded.join().unwrap(),
        InstanceLaunch::Forwarded
    ));
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
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let threads = ["first.md", "second.md"].map(|name| {
        let lock = lock.clone();
        let socket = socket.clone();
        let barrier = barrier.clone();
        let result_tx = result_tx.clone();
        std::thread::spawn(move || {
            barrier.wait();
            let result = acquire_with_paths(&lock, &socket, &[PathBuf::from(name)]).unwrap();
            result_tx.send(result).unwrap();
        })
    });
    barrier.wait();
    drop(result_tx);
    let first = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("one start must own the instance");
    let (guard, mut receiver) = match first {
        InstanceLaunch::Primary { guard, receiver } => (guard, receiver),
        InstanceLaunch::Forwarded => panic!("the primary must be reported before its forwarder"),
    };
    let request = futures::executor::block_on(receiver.next()).unwrap();
    assert_eq!(request.message.paths.len(), 1);
    assert!(matches!(
        request.message.paths[0].to_string_lossy().as_ref(),
        "first.md" | "second.md"
    ));
    request.respond(true);
    let second = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("the forwarded start must finish after UI acceptance");
    assert!(matches!(second, InstanceLaunch::Forwarded));
    for thread in threads {
        thread.join().unwrap();
    }
    drop(guard);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn completed_request_id_cache_is_bounded_and_deduplicates() {
    let mut completed = HashSet::new();
    let mut order = VecDeque::new();
    remember_completed_request(&mut completed, &mut order, 11);
    remember_completed_request(&mut completed, &mut order, 11);
    assert_eq!(completed.len(), 1);
    assert_eq!(order.len(), 1);
    for request_id in 0..=MAX_COMPLETED_REQUEST_IDS as u64 {
        remember_completed_request(&mut completed, &mut order, request_id + 100);
    }
    assert!(completed.len() <= MAX_COMPLETED_REQUEST_IDS);
    assert!(order.len() <= MAX_COMPLETED_REQUEST_IDS);
}
