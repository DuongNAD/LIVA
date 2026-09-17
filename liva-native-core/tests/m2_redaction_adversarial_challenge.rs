//! Adversarial Stress & Compliance Challenge Test Suite for Milestone M2
//! (Vietnamese PII Redaction & Decree 13 Compliance).
//!
//! Empirically stress-tests:
//! 1. Vietnamese Citizen Identity Cards (CCCD):
//!    - 12 digits starting with 0 in diverse syntactic contexts (quotes, punctuation, markdown, URLs).
//!    - Rejection of 11-digit or 13-digit numbers and non-zero prefixes.
//! 2. Vietnamese Mobile Phones:
//!    - Variations with +84, 0, and all major mobile prefixes (03x, 05x, 07x, 08x, 09x).
//!    - Rejection of landline numbers, short/long numbers, and non-VN numbers.
//!    - Empirical investigation of hyphen/space-separated formats.
//! 3. Vietnamese Bank Account Numbers:
//!    - Context-bound matching across keywords (stk, số tài khoản, tài khoản, chuyển khoản, account_number, iban).
//!    - Delimiters (colons, equals, hyphens, quotes, whitespace).
//!    - Length boundaries (9-16 digits) and non-context preservation.
//!    - Precedence disambiguation against CCCD and phone patterns.
//! 4. Anti-False-Positive Verification:
//!    - Timestamps (millisecond, second, ISO 8601).
//!    - Monetary amounts (VND with commas/dots).
//!    - Transaction IDs, Git commit SHAs, UUIDs, IP addresses.
//! 5. Composite JSON and Multiline Documents:
//!    - Nested structures, arrays, objects, multiline customer chat logs.
//! 6. Empirical Adversarial Challenge Findings:
//!    - Hyphen/space phone number bypass.
//!    - Markdown underscore boundary evasion.
//!    - Synthetic UUID collision with credit card and CCCD regexes.
//!    - Asymmetric JSON key matching (prefix vs suffix).

use liva_native_core::cognitive::SecretScrubber;
use serde_json::json;

// ============================================================================
// 1. CCCD Stress Tests (12 digits, starting with 0)
// ============================================================================

#[test]
fn test_challenge_cccd_syntactic_boundaries() {
    let valid_cccd = "001098012345";

    // Double and single quotes
    let in_quotes = format!(r#"Hồ sơ khách hàng: "{valid_cccd}" và '{valid_cccd}'"#);
    let scrubbed = SecretScrubber::scrub(&in_quotes);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        r#"Hồ sơ khách hàng: "[REDACTED_CCCD]" và '[REDACTED_CCCD]'"#
    );

    // Parentheses, brackets, braces, angle brackets
    let in_brackets =
        format!("CCCD: ({valid_cccd}), [{valid_cccd}], {{{valid_cccd}}}, <{valid_cccd}>");
    let scrubbed = SecretScrubber::scrub(&in_brackets);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        "CCCD: ([REDACTED_CCCD]), [[REDACTED_CCCD]], {[REDACTED_CCCD]}, <[REDACTED_CCCD]>"
    );

    // Sentence punctuation: period, comma, colon, semicolon, exclamation
    let in_punct = format!(
        "Số {valid_cccd}. Số {valid_cccd}, số {valid_cccd}; số {valid_cccd}: số {valid_cccd}!"
    );
    let scrubbed = SecretScrubber::scrub(&in_punct);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        "Số [REDACTED_CCCD]. Số [REDACTED_CCCD], số [REDACTED_CCCD]; số [REDACTED_CCCD]: số [REDACTED_CCCD]!"
    );

    // Markdown syntax: bold, italic asterisks, strikethrough, inline code
    let in_markdown = format!("**{valid_cccd}** *{valid_cccd}* ~~{valid_cccd}~~ `{valid_cccd}`");
    let scrubbed = SecretScrubber::scrub(&in_markdown);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        "**[REDACTED_CCCD]** *[REDACTED_CCCD]* ~~[REDACTED_CCCD]~~ `[REDACTED_CCCD]`"
    );

    // URLs: path parameter, query parameter, hash fragment
    let in_url_path = format!("https://dichvucong.gov.vn/api/v1/profile/{valid_cccd}");
    let scrubbed = SecretScrubber::scrub(&in_url_path);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        "https://dichvucong.gov.vn/api/v1/profile/[REDACTED_CCCD]"
    );

    let in_url_query = format!("https://example.com/api?cccd={valid_cccd}&action=verify");
    let scrubbed = SecretScrubber::scrub(&in_url_query);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(
        scrubbed,
        "https://example.com/api?cccd=[REDACTED_CCCD]&action=verify"
    );

    let in_url_hash = format!("https://example.com/check#{valid_cccd}");
    let scrubbed = SecretScrubber::scrub(&in_url_hash);
    assert!(!scrubbed.contains(valid_cccd));
    assert_eq!(scrubbed, "https://example.com/check#[REDACTED_CCCD]");
}

#[test]
fn test_challenge_cccd_length_and_prefix_rejection() {
    // 11 digits starting with 0: MUST NOT be redacted as CCCD
    let cccd_11 = "Mã số định danh 00109801234 có 11 chữ số";
    assert_eq!(SecretScrubber::scrub(cccd_11), cccd_11);

    // 13 digits starting with 0: MUST NOT be redacted as CCCD
    let cccd_13 = "Mã số định danh 0010980123456 có 13 chữ số";
    assert_eq!(SecretScrubber::scrub(cccd_13), cccd_13);

    // 10 digits starting with 0 (not a mobile number): MUST NOT be redacted as CCCD
    let cccd_10 = "Mã số 0123456789 có 10 chữ số";
    assert_eq!(SecretScrubber::scrub(cccd_10), cccd_10);

    // 12 digits NOT starting with 0: MUST NOT be redacted as CCCD
    let non_zero_prefixes = [
        "101098012345",
        "201098012345",
        "301098012345",
        "401098012345",
        "501098012345",
        "601098012345",
        "701098012345",
        "801098012345",
        "999098012345",
    ];
    for non_zero in non_zero_prefixes {
        let input = format!("Số hợp đồng: {non_zero} bảo hiểm");
        assert_eq!(
            SecretScrubber::scrub(&input),
            input,
            "12-digit number '{non_zero}' with non-zero prefix must NOT be redacted as CCCD"
        );
    }

    // 12 characters alphanumeric (e.g. tracking code)
    let alpha_12 = "Tracking: 00109801234A và B001098012345";
    assert_eq!(SecretScrubber::scrub(alpha_12), alpha_12);
}

// ============================================================================
// 2. Vietnamese Mobile Phone Stress Tests (03x, 05x, 07x, 08x, 09x, +84)
// ============================================================================

#[test]
fn test_challenge_vietnamese_mobile_all_prefixes() {
    // 03x: Viettel range
    let p_032 = "SĐT 0321234567 liên hệ";
    let p_038 = "SĐT 0389876543 liên hệ";
    let p_039 = "SĐT 0399887766 liên hệ";
    assert_eq!(SecretScrubber::scrub(p_032), "SĐT [REDACTED_PHONE] liên hệ");
    assert_eq!(SecretScrubber::scrub(p_038), "SĐT [REDACTED_PHONE] liên hệ");
    assert_eq!(SecretScrubber::scrub(p_039), "SĐT [REDACTED_PHONE] liên hệ");

    // 05x: Vietnamobile / Gmobile range
    let p_052 = "Gọi 0521234567 nha";
    let p_056 = "Gọi 0561234567 nha";
    let p_058 = "Gọi 0581234567 nha";
    let p_059 = "Gọi 0591234567 nha";
    assert_eq!(SecretScrubber::scrub(p_052), "Gọi [REDACTED_PHONE] nha");
    assert_eq!(SecretScrubber::scrub(p_056), "Gọi [REDACTED_PHONE] nha");
    assert_eq!(SecretScrubber::scrub(p_058), "Gọi [REDACTED_PHONE] nha");
    assert_eq!(SecretScrubber::scrub(p_059), "Gọi [REDACTED_PHONE] nha");

    // 07x: MobiFone range
    let p_070 = "Di động: 0701234567.";
    let p_076 = "Di động: 0761234567.";
    let p_077 = "Di động: 0771234567.";
    let p_078 = "Di động: 0781234567.";
    let p_079 = "Di động: 0791234567.";
    assert_eq!(SecretScrubber::scrub(p_070), "Di động: [REDACTED_PHONE].");
    assert_eq!(SecretScrubber::scrub(p_076), "Di động: [REDACTED_PHONE].");
    assert_eq!(SecretScrubber::scrub(p_077), "Di động: [REDACTED_PHONE].");
    assert_eq!(SecretScrubber::scrub(p_078), "Di động: [REDACTED_PHONE].");
    assert_eq!(SecretScrubber::scrub(p_079), "Di động: [REDACTED_PHONE].");

    // 08x: VinaPhone / Viettel / MobiFone range
    let p_081 = "Hotline 0811234567 trực 24/7";
    let p_083 = "Hotline 0831234567 trực 24/7";
    let p_085 = "Hotline 0851234567 trực 24/7";
    let p_088 = "Hotline 0881234567 trực 24/7";
    let p_089 = "Hotline 0891234567 trực 24/7";
    assert_eq!(
        SecretScrubber::scrub(p_081),
        "Hotline [REDACTED_PHONE] trực 24/7"
    );
    assert_eq!(
        SecretScrubber::scrub(p_083),
        "Hotline [REDACTED_PHONE] trực 24/7"
    );
    assert_eq!(
        SecretScrubber::scrub(p_085),
        "Hotline [REDACTED_PHONE] trực 24/7"
    );
    assert_eq!(
        SecretScrubber::scrub(p_088),
        "Hotline [REDACTED_PHONE] trực 24/7"
    );
    assert_eq!(
        SecretScrubber::scrub(p_089),
        "Hotline [REDACTED_PHONE] trực 24/7"
    );

    // 09x: Standard mobile range
    let p_090 = "Số: 0901234567";
    let p_091 = "Số: 0912345678";
    let p_094 = "Số: 0941234567";
    let p_097 = "Số: 0971234567";
    let p_098 = "Số: 0981234567";
    assert_eq!(SecretScrubber::scrub(p_090), "Số: [REDACTED_PHONE]");
    assert_eq!(SecretScrubber::scrub(p_091), "Số: [REDACTED_PHONE]");
    assert_eq!(SecretScrubber::scrub(p_094), "Số: [REDACTED_PHONE]");
    assert_eq!(SecretScrubber::scrub(p_097), "Số: [REDACTED_PHONE]");
    assert_eq!(SecretScrubber::scrub(p_098), "Số: [REDACTED_PHONE]");

    // +84 international format
    let p_int_03 = "+84381234567 là hotline";
    let p_int_05 = "+84581234567 là hotline";
    let p_int_07 = "+84781234567 là hotline";
    let p_int_08 = "+84881234567 là hotline";
    let p_int_09 = "+84912345678 là hotline";
    assert_eq!(
        SecretScrubber::scrub(p_int_03),
        "[REDACTED_PHONE] là hotline"
    );
    assert_eq!(
        SecretScrubber::scrub(p_int_05),
        "[REDACTED_PHONE] là hotline"
    );
    assert_eq!(
        SecretScrubber::scrub(p_int_07),
        "[REDACTED_PHONE] là hotline"
    );
    assert_eq!(
        SecretScrubber::scrub(p_int_08),
        "[REDACTED_PHONE] là hotline"
    );
    assert_eq!(
        SecretScrubber::scrub(p_int_09),
        "[REDACTED_PHONE] là hotline"
    );
}

#[test]
fn test_challenge_vietnamese_phone_rejection() {
    // Fixed/landline numbers (11 digits, area codes 024, 028)
    let landline_hn = "Văn phòng Hà Nội: 02438255555 tiếp khách";
    assert_eq!(SecretScrubber::scrub(landline_hn), landline_hn);

    let landline_hcm = "Văn phòng TP.HCM: 02838222222 tiếp khách";
    assert_eq!(SecretScrubber::scrub(landline_hcm), landline_hcm);

    // Invalid mobile prefixes: 01, 04, 06
    let invalid_prefix_01 = "Số nội bộ 0112345678 máy nhánh";
    assert_eq!(SecretScrubber::scrub(invalid_prefix_01), invalid_prefix_01);

    let invalid_prefix_04 = "Số trạm 0412345678 điều hành";
    assert_eq!(SecretScrubber::scrub(invalid_prefix_04), invalid_prefix_04);

    let invalid_prefix_06 = "Số dự phòng 0612345678";
    assert_eq!(SecretScrubber::scrub(invalid_prefix_06), invalid_prefix_06);

    // Short numbers (9 digits starting with 09)
    let short_phone = "Số ngắn 091234567 không đủ độ dài";
    assert_eq!(SecretScrubber::scrub(short_phone), short_phone);

    // Long numbers (11 digits starting with 09)
    let long_phone = "Số dài 09123456789 vượt quá chuẩn 10 số";
    assert_eq!(SecretScrubber::scrub(long_phone), long_phone);

    // Foreign numbers
    let us_number = "Call US office at +14155552671 immediately";
    assert_eq!(SecretScrubber::scrub(us_number), us_number);

    let uk_number = "Call UK office at +442071838750 immediately";
    assert_eq!(SecretScrubber::scrub(uk_number), uk_number);

    let jp_number = "Call Japan office at +81312345678 immediately";
    assert_eq!(SecretScrubber::scrub(jp_number), jp_number);
}

// ============================================================================
// 3. Vietnamese Bank Account Stress Tests (context-bound, 9-16 digits)
// ============================================================================

#[test]
fn test_challenge_bank_account_context_keywords_and_delimiters() {
    let acc_12 = "123456789012";

    // Keywords with colons
    let keywords_colon = [
        "STK: 123456789012",
        "stk: 123456789012",
        "Số tài khoản: 123456789012",
        "so tai khoan: 123456789012",
        "Tài khoản: 123456789012",
        "tai khoan: 123456789012",
        "Tài khoản số: 123456789012",
        "tai khoan so: 123456789012",
        "Chuyển khoản: 123456789012",
        "chuyen khoan: 123456789012",
        "tk nh: 123456789012",
        "tk ngân hàng: 123456789012",
        "account_number: 123456789012",
        "account no: 123456789012",
        "account num: 123456789012",
        "acct no: 123456789012",
        "acct num: 123456789012",
        "bank_account: 123456789012",
        "iban: 123456789012",
    ];

    for kw in keywords_colon {
        let scrubbed = SecretScrubber::scrub(kw);
        assert!(
            !scrubbed.contains(acc_12),
            "Bank account in '{kw}' must be redacted"
        );
        assert!(
            scrubbed.contains("[REDACTED_BANK_ACCOUNT]"),
            "Redacted text '{scrubbed}' must contain [REDACTED_BANK_ACCOUNT]"
        );
    }

    // Delimiters: equal sign, hyphen, space, quoted
    let delimiters = [
        ("stk = 123456789012", "stk = [REDACTED_BANK_ACCOUNT]"),
        ("STK - 123456789012", "STK - [REDACTED_BANK_ACCOUNT]"),
        (
            "số tài khoản 123456789012",
            "số tài khoản [REDACTED_BANK_ACCOUNT]",
        ),
        (
            r#"stk: "123456789012""#,
            r#"stk: "[REDACTED_BANK_ACCOUNT]""#,
        ),
        (
            r#"account_number='123456789012'"#,
            r#"account_number='[REDACTED_BANK_ACCOUNT]'"#,
        ),
    ];

    for (raw, expected) in delimiters {
        let scrubbed = SecretScrubber::scrub(raw);
        assert_eq!(
            scrubbed, expected,
            "Mismatch for delimiter pattern: '{raw}'"
        );
    }
}

#[test]
fn test_challenge_bank_account_length_boundaries() {
    // 9 digits (minimum valid bank account length)
    let min_9 = "STK: 123456789";
    assert_eq!(SecretScrubber::scrub(min_9), "STK: [REDACTED_BANK_ACCOUNT]");

    // 16 digits (maximum valid bank account length)
    let max_16 = "Số tài khoản: 1234567890123456";
    assert_eq!(
        SecretScrubber::scrub(max_16),
        "Số tài khoản: [REDACTED_BANK_ACCOUNT]"
    );

    // 8 digits (under minimum length) -> MUST NOT be redacted
    let under_8 = "STK: 12345678";
    assert_eq!(SecretScrubber::scrub(under_8), under_8);

    // 17 digits (over maximum length) -> MUST NOT be redacted
    let over_17 = "STK: 12345678901234567";
    assert_eq!(SecretScrubber::scrub(over_17), over_17);

    // Standalone number without bank context -> MUST NOT be redacted
    let standalone_14 = "Mã bưu điện quốc tế 12345678901234 trên bưu phẩm";
    assert_eq!(SecretScrubber::scrub(standalone_14), standalone_14);
}

#[test]
fn test_challenge_bank_account_disambiguation_and_precedence() {
    // A 12-digit bank account starting with 0:
    // MUST be redacted as [REDACTED_BANK_ACCOUNT], NOT [REDACTED_CCCD]
    let bank_starts_with_0 = "Vui lòng gửi tiền vào STK: 001098012345 chi nhánh Đống Đa";
    let scrubbed_0 = SecretScrubber::scrub(bank_starts_with_0);
    assert!(!scrubbed_0.contains("001098012345"));
    assert!(
        scrubbed_0.contains("STK: [REDACTED_BANK_ACCOUNT]"),
        "Precedence failure: 12-digit bank account starting with 0 was not redacted as bank account: {scrubbed_0}"
    );
    assert!(
        !scrubbed_0.contains("[REDACTED_CCCD]"),
        "Precedence failure: bank account falsely tagged as CCCD: {scrubbed_0}"
    );

    // A 10-digit bank account starting with 09:
    // MUST be redacted as [REDACTED_BANK_ACCOUNT], NOT [REDACTED_PHONE]
    let bank_starts_with_09 = "Số tài khoản: 0912345678 tại Vietcombank";
    let scrubbed_09 = SecretScrubber::scrub(bank_starts_with_09);
    assert!(!scrubbed_09.contains("0912345678"));
    assert!(
        scrubbed_09.contains("Số tài khoản: [REDACTED_BANK_ACCOUNT]"),
        "Precedence failure: 10-digit bank account starting with 09 was not redacted as bank account: {scrubbed_09}"
    );
    assert!(
        !scrubbed_09.contains("[REDACTED_PHONE]"),
        "Precedence failure: bank account falsely tagged as phone: {scrubbed_09}"
    );
}

// ============================================================================
// 4. Anti-False-Positive Verification (Timestamps, Money, IDs, Hashes)
// ============================================================================

#[test]
fn test_challenge_anti_false_positives() {
    // Unix millisecond timestamps (13 digits starting with 1)
    let ts_ms = "Event recorded at timestamp 1726000000000 and updated at 1726176000000 ms";
    assert_eq!(SecretScrubber::scrub(ts_ms), ts_ms);

    // Unix second timestamps (10 digits starting with 1)
    let ts_sec = "Session expires_at: 1726000000 unix seconds";
    assert_eq!(SecretScrubber::scrub(ts_sec), ts_sec);

    // ISO 8601 timestamps
    let iso_ts = "Audit time: 2026-09-13T06:30:16Z with offset +07:00";
    assert_eq!(SecretScrubber::scrub(iso_ts), iso_ts);

    // Monetary amounts with comma/dot formatting
    let amounts = [
        "Tổng thanh toán: 50,000,000 VND qua cổng Napas",
        "Số tiền chuyển khoản: 50,000,000 VND",
        "Chi phí dịch vụ 50.000.000 VND",
        "Giá niêm yết: 100,000,000 đ",
        "Số dư ví: 500,000,000 VND",
    ];
    for amt in amounts {
        assert_eq!(
            SecretScrubber::scrub(amt),
            amt,
            "Monetary amount falsely redacted in: '{amt}'"
        );
    }

    // Transaction IDs and Reference Numbers
    let txn_ids = [
        "Transaction ID: TXN-987654321012 hoàn tất",
        "Reference code REF_20260913_0001_A9",
        "Order number ORD#8877665544 đã thanh toán",
        "Invoice code INV-2026-09-8812",
    ];
    for txn in txn_ids {
        assert_eq!(SecretScrubber::scrub(txn), txn);
    }

    // Git commit SHAs (40-char hex)
    let git_sha = "Commit hash 4b825dc642cb6eb9a060e54bf8d69288fbee4904 verified";
    assert_eq!(SecretScrubber::scrub(git_sha), git_sha);

    let git_sha_zero_prefix = "Commit 0123456789abcdef0123456789abcdef01234567 in main";
    assert_eq!(
        SecretScrubber::scrub(git_sha_zero_prefix),
        git_sha_zero_prefix
    );

    // Standard UUIDs (36 characters with standard random hex components)
    let uuid_1 = "Request-ID: 550e8400-e29b-41d4-a716-446655440000";
    assert_eq!(SecretScrubber::scrub(uuid_1), uuid_1);

    let uuid_2 = "Correlation ID c0a80101-4f3b-41d4-a716-446655440001 dispatched";
    assert_eq!(SecretScrubber::scrub(uuid_2), uuid_2);

    // Networking, IP addresses and ports
    let net = "Server listening on http://127.0.0.1:8080 and https://192.168.1.1:3000";
    assert_eq!(SecretScrubber::scrub(net), net);
}

// ============================================================================
// 5. Composite JSON and Multiline Documents
// ============================================================================

#[test]
fn test_challenge_composite_json_deep_hierarchy() {
    let payload = json!({
        "status": "success",
        "code": 200,
        "metadata": {
            "created_at_ms": 1726176000000_i64,
            "request_id": "550e8400-e29b-41d4-a716-446655440000",
            "active": true,
            "balance": 50000000
        },
        "customer": {
            "name": "Tran Van B",
            "cccd": "001098012345",
            "cmnd": "001098012345",
            "phone": "0987654321",
            "stk": "123456789012",
            "dest_stk": "999888777666",
            "bank_account": "987654321012",
            "account_number": "112233445566",
            "api_key": "sk-proj-12345678901234567890",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0...\n-----END RSA PRIVATE KEY-----"
        },
        "transactions": [
            {
                "txn_id": "TXN-99887766",
                "dest_stk": "999888777666",
                "amount_vnd": "50,000,000 VND",
                "memo": "Chuyển tiền cho SĐT 0912345678 theo STK: 123456789012"
            }
        ]
    });

    let scrubbed = SecretScrubber::scrub_json(&payload);

    // Non-PII preserved
    assert_eq!(scrubbed["status"], "success");
    assert_eq!(scrubbed["code"], 200);
    assert_eq!(scrubbed["metadata"]["created_at_ms"], 1726176000000_i64);
    assert_eq!(
        scrubbed["metadata"]["request_id"],
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(scrubbed["metadata"]["active"], true);
    assert_eq!(scrubbed["metadata"]["balance"], 50000000);
    assert_eq!(scrubbed["customer"]["name"], "Tran Van B");

    // Keys and values scrubbed
    assert_eq!(scrubbed["customer"]["cccd"], "[REDACTED_CCCD]");
    assert_eq!(scrubbed["customer"]["cmnd"], "[REDACTED_CCCD]");
    assert_eq!(scrubbed["customer"]["phone"], "[REDACTED_PHONE]");
    assert_eq!(scrubbed["customer"]["stk"], "[REDACTED_BANK_ACCOUNT]");
    assert_eq!(scrubbed["customer"]["dest_stk"], "[REDACTED_BANK_ACCOUNT]");
    assert_eq!(
        scrubbed["customer"]["bank_account"],
        "[REDACTED_BANK_ACCOUNT]"
    );
    assert_eq!(
        scrubbed["customer"]["account_number"],
        "[REDACTED_BANK_ACCOUNT]"
    );
    assert_eq!(scrubbed["customer"]["api_key"], "[REDACTED_SECRET]");
    assert_eq!(scrubbed["customer"]["private_key"], "[REDACTED_SECRET]");

    // Array items scrubbed
    assert_eq!(scrubbed["transactions"][0]["txn_id"], "TXN-99887766");
    assert_eq!(
        scrubbed["transactions"][0]["dest_stk"],
        "[REDACTED_BANK_ACCOUNT]"
    );
    assert_eq!(scrubbed["transactions"][0]["amount_vnd"], "50,000,000 VND");
    assert_eq!(
        scrubbed["transactions"][0]["memo"],
        "Chuyển tiền cho SĐT [REDACTED_PHONE] theo STK: [REDACTED_BANK_ACCOUNT]"
    );
}

#[test]
fn test_challenge_multiline_audit_log_document() {
    let multiline_doc = r#"
=== BÁO CÁO KIỂM TOÁN HỆ THỐNG LIVA ===
Thời gian: 2026-09-13T06:30:16Z
Phiên làm việc: sess_audit_alpha_beta_gamma
Mã tiến trình: 4512

Thông tin người dùng:
- Họ tên: Le Thi C
- Số CCCD: 079199000123 (Cấp tại TP.HCM)
- Số điện thoại: 0903123456 hoặc hotline +84903123456
- Tài khoản ngân hàng MB: STK: 0680123456789
- Khóa truy cập API: sk-ant-api03-12345678901234567890

Lịch sử giao dịch gần nhất:
| ID Giao dịch | Thời điểm | Số tiền | STK thụ hưởng | Trạng thái |
| TXN-001 | 1726176000000 | 15,000,000 VND | STK: 998877665544 | Thành công |
| TXN-002 | 1726176300000 | 2,500,000 VND | STK: 887766554433 | Thành công |

Ghi chú: Khách hàng yêu cầu gửi OTP về 0903123456 khi đăng nhập.
"#;

    let scrubbed = SecretScrubber::scrub(multiline_doc);

    // Verify all PII removed
    assert!(!scrubbed.contains("079199000123"));
    assert!(!scrubbed.contains("0903123456"));
    assert!(!scrubbed.contains("+84903123456"));
    assert!(!scrubbed.contains("0680123456789"));
    assert!(!scrubbed.contains("sk-ant-api03-12345678901234567890"));
    assert!(!scrubbed.contains("998877665544"));
    assert!(!scrubbed.contains("887766554433"));

    // Verify redactions in place
    assert!(scrubbed.contains("Số CCCD: [REDACTED_CCCD]"));
    assert!(scrubbed.contains("Số điện thoại: [REDACTED_PHONE] hoặc hotline [REDACTED_PHONE]"));
    assert!(scrubbed.contains("STK: [REDACTED_BANK_ACCOUNT]"));
    assert!(scrubbed.contains("[REDACTED_ANTHROPIC_KEY]"));

    // Verify non-PII preserved
    assert!(scrubbed.contains("2026-09-13T06:30:16Z"));
    assert!(scrubbed.contains("TXN-001"));
    assert!(scrubbed.contains("TXN-002"));
    assert!(scrubbed.contains("1726176000000"));
    assert!(scrubbed.contains("1726176300000"));
    assert!(scrubbed.contains("15,000,000 VND"));
    assert!(scrubbed.contains("2,500,000 VND"));
}

// ============================================================================
// 6. Empirical Adversarial Stress & Vulnerability Probing
// ============================================================================

#[test]
fn test_adversarial_hyphen_and_space_phone_behavior() {
    // 1. Contiguous mobile numbers ARE correctly redacted
    assert_eq!(
        SecretScrubber::scrub("Tel: 0912345678"),
        "Tel: [REDACTED_PHONE]"
    );
    assert_eq!(
        SecretScrubber::scrub("Tel: +84912345678"),
        "Tel: [REDACTED_PHONE]"
    );

    // 2. EMPIRICAL VULNERABILITY FINDING:
    // When users/transcriptions provide phone numbers with hyphens or spaces,
    // RE_VIETNAMESE_PHONE = r"(?:\+84|\b0)(?:3|5|7|8|9)\d{8}\b" fails to match
    // because \d{8} demands contiguous digits.
    let hyphenated = "Tel: 091-234-5678";
    let scrubbed_hyphen = SecretScrubber::scrub(hyphenated);
    assert_eq!(
        scrubbed_hyphen, hyphenated,
        "Confirmed limitation: Hyphenated phone numbers bypass redaction"
    );

    let spaced = "Tel: 091 234 5678";
    let scrubbed_spaced = SecretScrubber::scrub(spaced);
    assert_eq!(
        scrubbed_spaced, spaced,
        "Confirmed limitation: Spaced phone numbers bypass redaction"
    );

    let int_hyphen = "Tel: +84-912-345-678";
    let scrubbed_int_hyphen = SecretScrubber::scrub(int_hyphen);
    assert_eq!(
        scrubbed_int_hyphen, int_hyphen,
        "Confirmed limitation: International hyphenated phone numbers bypass redaction"
    );
}

#[test]
fn test_adversarial_markdown_underscore_boundary() {
    // 1. Asterisk markdown italics *001098012345* DOES match (\b is valid between * and 0)
    let asterisk_cccd = "Số CCCD: *001098012345* trong văn bản";
    assert_eq!(
        SecretScrubber::scrub(asterisk_cccd),
        "Số CCCD: *[REDACTED_CCCD]* trong văn bản"
    );

    // 2. EMPIRICAL VULNERABILITY FINDING:
    // Markdown italics using underscores (_001098012345_) fails to match
    // because '_' is an ASCII word character (\w). Thus, between '_' and '0'
    // there is NO \b word boundary.
    let underscore_cccd = "Số CCCD: _001098012345_ trong văn bản";
    let scrubbed_underscore = SecretScrubber::scrub(underscore_cccd);
    assert_eq!(
        scrubbed_underscore, underscore_cccd,
        "Confirmed limitation: Underscore-wrapped CCCD numbers bypass regex word boundary"
    );
}

#[test]
fn test_adversarial_uuid_with_numeric_suffix() {
    // 1. Standard random UUIDs with letters are preserved
    let standard_uuid = "UUID: 550e8400-e29b-41d4-a716-446655440000";
    assert_eq!(SecretScrubber::scrub(standard_uuid), standard_uuid);

    // 2. EMPIRICAL VULNERABILITY FINDING (False Positive):
    // A synthetic or sequential UUID whose trailing 12 characters are purely digits starting with 0
    // (e.g. 550e8400-e29b-41d4-a716-001098012345) gets falsely redacted as [REDACTED_CCCD]
    // because the hyphen '-' is \W and '0' is \w, forming a word boundary.
    let uuid_with_0_suffix = "UUID: 550e8400-e29b-41d4-a716-001098012345";
    let scrubbed_uuid = SecretScrubber::scrub(uuid_with_0_suffix);
    assert_eq!(
        scrubbed_uuid, "UUID: 550e8400-e29b-41d4-a716-[REDACTED_CCCD]",
        "Confirmed edge case: UUIDs ending in 12 digits starting with 0 collide with CCCD regex"
    );
}

#[test]
fn test_adversarial_json_asymmetric_key_detection() {
    // Suffix keys like "dest_stk", "customer_stk" ARE detected:
    let json_suffix = json!({
        "dest_stk": "123456789012"
    });
    let scrubbed_suffix = SecretScrubber::scrub_json(&json_suffix);
    assert_eq!(scrubbed_suffix["dest_stk"], "[REDACTED_BANK_ACCOUNT]");

    // EMPIRICAL VULNERABILITY FINDING:
    // Prefix keys like "stk_dest", "stk_nhan", "account_no" are NOT detected
    // because scrub_json only checks key_lower.ends_with("_stk").
    // When the raw value has no "STK:" prefix, it bypasses scrub() and leaks!
    let json_prefix = json!({
        "stk_dest": "123456789012"
    });
    let scrubbed_prefix = SecretScrubber::scrub_json(&json_prefix);
    assert_eq!(
        scrubbed_prefix["stk_dest"], "123456789012",
        "Confirmed limitation: Prefix keys like 'stk_dest' bypass JSON scrubbing when raw string lacks context"
    );
}
