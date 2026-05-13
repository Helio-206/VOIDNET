use void_transport::{
    LifecycleEngine, NodeLifecycleState, PeerRuntimeInfo, PeerTopology, TransportHealth,
};

#[test]
fn lifecycle_reaches_active_after_transport_path() {
    let mut lifecycle = LifecycleEngine::new();

    lifecycle.transition(NodeLifecycleState::Bootstrap, "identity loaded");
    lifecycle.transition(NodeLifecycleState::Discovering, "listener active");
    lifecycle.transition(NodeLifecycleState::Authenticating, "connection established");
    lifecycle.transition(NodeLifecycleState::Syncing, "peer authenticated");
    lifecycle.transition(NodeLifecycleState::Active, "ping observed active peer");

    assert_eq!(lifecycle.state(), NodeLifecycleState::Active);
    assert_eq!(lifecycle.history().len(), 5);
}

#[test]
fn topology_tracks_active_peer_latency() {
    let mut topology = PeerTopology::new("local");

    topology.observe_discovered("peer-a", vec!["/ip4/127.0.0.1/udp/10000/quic-v1".into()]);
    topology.observe_connected("peer-a", None, "quic-v1");
    topology.observe_authenticated("peer-a");
    topology.observe_runtime_state(
        "peer-a",
        PeerRuntimeInfo::new(
            "voidnet/0.1.0",
            NodeLifecycleState::Syncing,
            5,
            vec!["runtime/mount".into()],
            TransportHealth::Healthy,
            false,
        ),
    );
    topology.observe_latency("peer-a", 41);

    assert_eq!(topology.active_peer_count(), 1);
    assert!(topology.render_table().contains("41ms"));
    assert!(topology.render_ascii().contains("peer-a"));
}

#[test]
fn topology_marks_partition_and_recovery() {
    let mut topology = PeerTopology::new("local");

    topology.observe_discovered("peer-a", vec!["/ip4/127.0.0.1/udp/10000/quic-v1".into()]);
    topology.observe_connected("peer-a", None, "quic-v1");
    topology.observe_authenticated("peer-a");
    topology.observe_transport_encryption("peer-a", "libp2p-quic");
    topology.observe_latency("peer-a", 41);
    assert_eq!(topology.active_peer_count(), 1);

    topology.observe_disconnected("peer-a");
    topology.mark_partitioned();
    assert!(topology.render_table().contains("PARTITIONED"));
    assert!(topology.render_sessions().contains("OFFLINE"));

    topology.observe_connected("peer-a", None, "quic-v1");
    topology.observe_authenticated("peer-a");
    topology.observe_latency("peer-a", 39);
    assert_eq!(topology.active_peer_count(), 1);
}
