use liva_native_core::agent::graph::{Intent, route_intent};
use std::time::Instant;

#[test]
fn test_reflex_lane_volume_control() {
    let cases = [
        ("tăng âm lượng", "control_volume", "up"),
        ("bật to loa lên", "control_volume", "up"),
        ("cho loa lớn hơn", "control_volume", "up"),
        ("tăng tiếng lên", "control_volume", "up"),
        ("cho nhỏ tiếng lại", "control_volume", "down"),
        ("giảm âm lượng", "control_volume", "down"),
        ("cho bé loa xuống", "control_volume", "down"),
        ("khẽ tiếng lại", "control_volume", "down"),
        ("tắt tiếng", "control_volume", "mute"),
        ("tắt loa đi", "control_volume", "mute"),
    ];

    for (input, expected_tool, expected_action) in cases {
        let intent = route_intent(input);
        assert_eq!(
            intent,
            Intent::OsControl {
                tool: expected_tool,
                action: expected_action
            },
            "Failed on input: '{input}'"
        );
    }
}

#[test]
fn test_reflex_lane_media_control() {
    let cases = [
        ("chuyển bài khác", "control_media", "next"),
        ("bài tiếp theo", "control_media", "next"),
        ("kế tiếp bài này", "control_media", "next"),
        ("quay lại bài trước", "control_media", "previous"),
        ("lùi bài hát lại", "control_media", "previous"),
        ("bài trước", "control_media", "previous"),
        ("dừng nhạc lại", "control_media", "play_pause"),
        ("phát bài hát", "control_media", "play_pause"),
        ("bật nhạc lên", "control_media", "play_pause"),
        ("mở bài hát này", "control_media", "play_pause"),
        ("tắt nhạc", "control_media", "play_pause"),
    ];

    for (input, expected_tool, expected_action) in cases {
        let intent = route_intent(input);
        assert_eq!(
            intent,
            Intent::OsControl {
                tool: expected_tool,
                action: expected_action
            },
            "Failed on input: '{input}'"
        );
    }
}

#[test]
fn test_reflex_lane_smart_home() {
    let cases = [
        ("bật đèn phòng khách", "light", "on"),
        ("tắt đèn đi", "light", "off"),
        ("mở đèn", "light", "on"),
        ("đóng đèn", "light", "off"),
        ("turn on light", "light", "on"),
        ("turn off lamp", "light", "off"),
        ("bật điều hoà", "ac", "on"),
        ("tắt điều hòa", "ac", "off"),
        ("mở máy lạnh lên", "ac", "on"),
        ("tắt máy lạnh", "ac", "off"),
        ("turn on ac", "ac", "on"),
        ("turn off ac", "ac", "off"),
        ("bật quạt", "fan", "on"),
        ("tắt quạt", "fan", "off"),
        ("mở quạt", "fan", "on"),
        ("turn on fan", "fan", "on"),
        ("turn off fan", "fan", "off"),
    ];

    for (input, expected_device, expected_action) in cases {
        let intent = route_intent(input);
        assert_eq!(
            intent,
            Intent::SmartHome {
                device: expected_device,
                action: expected_action
            },
            "Failed on input: '{input}'"
        );
    }
}

#[test]
fn test_reflex_lane_message_drafting() {
    let cases = [
        (
            "nhắn tin cho Minh Hiến bảo nó ngủ đi",
            "Minh Hiến",
            "ngủ đi",
            None,
        ),
        (
            "gửi tin nhắn cho Nam bảo chuẩn bị họp",
            "Nam",
            "chuẩn bị họp",
            None,
        ),
        (
            "nhắn tin cho Linh qua Telegram bảo gọi lại cho anh",
            "Linh",
            "gọi lại cho anh",
            Some("telegram"),
        ),
        (
            "gửi cho Hoàng bằng Messenger là mai họp lúc 9h",
            "Hoàng",
            "mai họp lúc 9h",
            Some("messenger"),
        ),
        ("nhắn cho Bảo", "Bảo", "", None),
    ];

    for (input, exp_recipient, exp_body, exp_platform) in cases {
        let intent = route_intent(input);
        assert_eq!(
            intent,
            Intent::SendMessage {
                recipient: exp_recipient.to_string(),
                body: exp_body.to_string(),
                platform: exp_platform.map(|s| s.to_string()),
            },
            "Failed on input: '{input}'"
        );
    }
}

#[test]
fn test_reflex_lane_vision_priority() {
    let cases = [
        "chụp màn hình",
        "nhìn trên màn hình",
        "xem màn hình này",
        "take a screenshot",
        "check the screen",
    ];

    for input in cases {
        let intent = route_intent(input);
        assert_eq!(intent, Intent::Vision, "Failed on input: '{input}'");
    }
}

#[test]
fn test_reflex_lane_chat_fallback_and_no_false_positives() {
    let cases = [
        "thời tiết hôm nay thế nào?",
        "giải thích thuật toán Dijkstra",
        "xin chào LIVA",
        "let's get back on track",
        "what is on the table",
        "how to configure coffee machine",
    ];

    for input in cases {
        let intent = route_intent(input);
        assert_eq!(intent, Intent::Chat, "Failed on input: '{input}'");
    }
}

#[test]
fn test_reflex_lane_sub_2ms_latency_and_throughput_benchmark() {
    let benchmark_queries = [
        "tăng âm lượng",
        "giảm âm lượng",
        "tắt tiếng",
        "chuyển bài khác",
        "quay lại bài trước",
        "dừng nhạc",
        "bật đèn phòng khách",
        "tắt máy lạnh",
        "bật quạt",
        "nhắn tin cho Minh Hiến bảo mai đi học",
        "gửi tin nhắn cho Nam qua Telegram bảo gọi lại",
        "chụp màn hình",
        "thời tiết hôm nay thế nào?",
        "hướng dẫn cài đặt rust trên windows",
    ];

    const ITERATIONS_PER_QUERY: usize = 1_000;
    let mut latencies_nanos = Vec::with_capacity(benchmark_queries.len() * ITERATIONS_PER_QUERY);

    // Warm up cache
    for query in &benchmark_queries {
        let _ = route_intent(query);
    }

    let start_total = Instant::now();
    for _ in 0..ITERATIONS_PER_QUERY {
        for query in &benchmark_queries {
            let t0 = Instant::now();
            let _ = route_intent(query);
            let elapsed = t0.elapsed();
            latencies_nanos.push(elapsed.as_nanos());
        }
    }
    let total_elapsed = start_total.elapsed();

    latencies_nanos.sort_unstable();
    let count = latencies_nanos.len();
    let min_ns = latencies_nanos[0];
    let p50_ns = latencies_nanos[count * 50 / 100];
    let p90_ns = latencies_nanos[count * 90 / 100];
    let p99_ns = latencies_nanos[count * 99 / 100];
    let max_ns = latencies_nanos[count - 1];

    let avg_ns = latencies_nanos.iter().sum::<u128>() / (count as u128);

    println!("\n=== REFLEX LANE ROUTER MICROBENCHMARK ===");
    println!("Total queries evaluated: {count} in {total_elapsed:?}");
    println!("Min latency: {:.3} µs", min_ns as f64 / 1_000.0);
    println!("Avg latency: {:.3} µs", avg_ns as f64 / 1_000.0);
    println!("P50 latency: {:.3} µs", p50_ns as f64 / 1_000.0);
    println!("P90 latency: {:.3} µs", p90_ns as f64 / 1_000.0);
    println!("P99 latency: {:.3} µs", p99_ns as f64 / 1_000.0);
    println!("Max latency: {:.3} µs", max_ns as f64 / 1_000.0);
    println!(
        "Throughput: {:.2} queries/sec",
        (count as f64) / total_elapsed.as_secs_f64()
    );
    println!("========================================\n");

    // Reflex Lane SLA guarantee: routing must take < 2ms (2,000,000 ns) with zero token cost
    assert!(
        p99_ns < 2_000_000,
        "Reflex Lane P99 latency exceeded 2ms SLA: got {} µs",
        p99_ns as f64 / 1_000.0
    );
    assert!(
        p50_ns < 500_000,
        "Reflex Lane P50 latency exceeded 500µs: got {} µs",
        p50_ns as f64 / 1_000.0
    );
}
