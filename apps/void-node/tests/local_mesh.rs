use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
#[ignore = "requires local UDP sockets and a built void-node binary"]
fn two_node_local_swarm_connects() {
    let mut node_a = spawn_node("voidnet-test-a", &[]).expect("spawn node a");
    let listen = wait_for_log(&mut node_a, "Listening address=", Duration::from_secs(10))
        .expect("node a listen address");
    let address = listen
        .split("Listening address=")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .expect("listen address in log")
        .to_string();

    let mut node_b =
        spawn_node("voidnet-test-b", &["--bootstrap", address.as_str()]).expect("spawn node b");
    let connected = wait_for_log(&mut node_b, "TransportConnected", Duration::from_secs(20));

    let _ = node_a.kill();
    let _ = node_b.kill();

    assert!(connected.is_some());
}

#[test]
#[ignore = "requires local UDP sockets and a built void-node binary"]
fn three_node_topology_forms_through_bootstrap() {
    let mut node_a = spawn_node("voidnet-test-topology-a", &[]).expect("spawn node a");
    let listen = wait_for_log(&mut node_a, "Listening address=", Duration::from_secs(10))
        .expect("node a listen address");
    let address = listen
        .split("Listening address=")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .expect("listen address in log")
        .to_string();

    let mut node_b =
        spawn_node("voidnet-test-topology-b", &["--bootstrap", address.as_str()])
            .expect("spawn node b");
    let mut node_c =
        spawn_node("voidnet-test-topology-c", &["--bootstrap", address.as_str()])
            .expect("spawn node c");

    let b_connected = wait_for_log(&mut node_b, "TransportConnected", Duration::from_secs(20));
    let c_connected = wait_for_log(&mut node_c, "TransportConnected", Duration::from_secs(20));

    let _ = node_a.kill();
    let _ = node_b.kill();
    let _ = node_c.kill();

    assert!(b_connected.is_some());
    assert!(c_connected.is_some());
}

fn spawn_node(name: &str, extra: &[&str]) -> std::io::Result<Child> {
    let data_dir = std::env::temp_dir().join(name);
    let mut command = Command::new(env!("CARGO_BIN_EXE_void-node"));
    command
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--listen")
        .arg("/ip4/127.0.0.1/udp/0/quic-v1")
        .arg("--exit-after-secs")
        .arg("30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for arg in extra {
        command.arg(arg);
    }
    command.spawn()
}

fn wait_for_log(child: &mut Child, needle: &str, timeout: Duration) -> Option<String> {
    let stdout = child.stdout.take()?;
    let needle = needle.to_string();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if line.contains(&needle) {
                let _ = tx.send(line);
                break;
            }
        }
    });

    rx.recv_timeout(timeout).ok()
}
