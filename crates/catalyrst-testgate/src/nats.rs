//! A test-owned loopback broker, isolated by an OS-selected port and reaped on drop.

use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread::JoinHandle,
    time::Duration,
};

pub struct Broker {
    child: Child,
    reader: Option<JoinHandle<()>>,
    pub port: u16,
}

impl Broker {
    pub fn start(binary: &str, port: Option<u16>) -> Self {
        let mut child = Command::new(binary)
            .args([
                "--addr",
                "127.0.0.1",
                "--port",
                &port.map(|p| p.to_string()).unwrap_or("-1".into()),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start test-owned NATS broker");
        let stderr = child.stderr.take().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if let Some((_, address)) = line.split_once("Listening for client connections on ")
                {
                    if let Ok(address) = address.trim().parse::<SocketAddr>() {
                        let _ = send.try_send(address.port());
                    }
                }
            }
        });
        let mut broker = Self {
            child,
            reader: Some(reader),
            port: 0,
        };
        broker.port = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("broker did not announce its loopback port");
        broker
    }

    pub fn url(&self) -> String {
        format!("nats://127.0.0.1:{}", self.port)
    }

    #[cfg(unix)]
    pub fn signal(&mut self, signal: &str) {
        assert!(
            matches!(signal, "-STOP" | "-CONT"),
            "only fixture pause/resume is supported"
        );
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "test broker already exited"
        );
        assert!(Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()
            .unwrap()
            .success());
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
