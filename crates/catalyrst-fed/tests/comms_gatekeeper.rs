use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use catalyrst_fed::comms::CommsGatekeeper;

fn spawn_participants_server(hits: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let hits = hits.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                hits.fetch_add(1, Ordering::SeqCst);
                let body = br#"{"data":{"addresses":["0xabc"]}}"#;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
            });
        }
    });
    format!("http://{addr}")
}

fn spawn_failing_server(hits: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let hits = hits.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                hits.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                );
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn scene_fetch_failure_is_negatively_cached() {
    let hits = Arc::new(AtomicUsize::new(0));
    let gk = CommsGatekeeper::new(spawn_failing_server(hits.clone()));

    assert!(gk.get_scene_participants("0,0").await.is_empty());
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    assert!(gk.get_scene_participants("0,0").await.is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "second call within the negative TTL must not refetch"
    );

    assert!(gk.get_scene_participants("1,1").await.is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "negative caching is per key, a different pointer still fetches"
    );
}

#[tokio::test]
async fn world_fetch_failure_is_negatively_cached() {
    let hits = Arc::new(AtomicUsize::new(0));
    let gk = CommsGatekeeper::new(spawn_failing_server(hits.clone()));

    assert!(gk.get_world_participants("foo.dcl.eth").await.is_empty());
    assert!(gk.get_world_participants("foo.dcl.eth").await.is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "second call within the negative TTL must not refetch"
    );
}

// Presence goes stale the moment somebody walks out, and a scene and a world
// expire independently, so the two must never be served from one another's
// entry: an avatar that left Genesis City for a world would otherwise stand in
// both rooms at once.
#[tokio::test]
async fn a_scene_and_a_world_sharing_an_identifier_are_kept_apart() {
    let hits = Arc::new(AtomicUsize::new(0));
    let gk = CommsGatekeeper::new(spawn_participants_server(hits.clone()));

    assert_eq!(gk.get_scene_participants("shared").await, vec!["0xabc"]);
    assert_eq!(gk.get_world_participants("shared").await, vec!["0xabc"]);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "the scene and the world must not share a cache entry"
    );

    assert_eq!(gk.get_scene_participants("shared").await, vec!["0xabc"]);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "a repeat inside the cache window still answers from the entry"
    );
}
