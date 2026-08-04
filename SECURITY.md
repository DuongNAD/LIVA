# Security Policy

## Supported versions

Security fixes are provided for the latest released version of LIVA. Older
development snapshots and archived Node.js/Python backends are unsupported.

| Version | Supported |
| --- | --- |
| 1.0.x | Yes |
| < 1.0 | No |

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's
**Security** tab and choose **Report a vulnerability** to submit a private
security advisory to the maintainers of
[DuongNAD/LIVA](https://github.com/DuongNAD/LIVA/security/advisories/new).

Include the affected version, operating system, reproduction steps, expected
impact, and any proof-of-concept material needed to confirm the issue. Do not
include real user secrets, memory-database contents, conversation transcripts,
or model credentials.

## Response targets

- Acknowledgement: within 3 business days.
- Initial assessment and severity: within 7 business days.
- Status update: at least every 14 days until resolution.

Resolution time depends on severity and release risk. The maintainers will
coordinate disclosure and credit with the reporter after a fix is available.

## Scope

In scope:

- The current Rust native core and Tauri desktop application.
- Command authorization, WebSocket session tickets, IPC boundaries, local data
  encryption, keystore handling, model/artifact integrity, and update/release
  packaging.
- Reproducible vulnerabilities in the latest release using supported
  configuration.

Out of scope:

- Archived or retired Node.js/Python backend code.
- Social engineering, denial of service requiring physical access, and reports
  that only identify an outdated dependency without a working impact path.
- Secrets or model files that a user intentionally exposes outside LIVA's data
  directory or supported credential storage.

## Safe harbor

Good-faith research that avoids privacy violations, data destruction, service
disruption, and access beyond what is necessary to demonstrate the issue is
welcome. Allow reasonable time for remediation before public disclosure.
