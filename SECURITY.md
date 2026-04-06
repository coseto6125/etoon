# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.2.x   | :white_check_mark: |
| < 0.2   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do NOT open a public GitHub issue.**
2. Email: **coseto6125@gmail.com** with subject `[etoon] Security Vulnerability`.
3. Include: description, reproduction steps, and impact assessment.
4. You will receive a response within 72 hours.

## Verification

All release binaries include:
- `SHA256SUMS.txt` — checksum verification
- [Sigstore cosign](https://www.sigstore.dev) keyless signatures (`.sig` + `.pem`) — provenance verification

See the [install instructions](README.md#install) for verification commands.
