# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.2.x   | :white_check_mark: |
| < 0.2   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it via [GitHub Private Vulnerability Reporting](https://github.com/coseto6125/etoon/security/advisories/new).

Please include: description, reproduction steps, and impact assessment.

## Verification

All release binaries include:
- `SHA256SUMS.txt` — checksum verification
- [SLSA provenance](https://slsa.dev) attestation — verify with `gh attestation verify`
- [VirusTotal](https://www.virustotal.com) scan reports (linked in release notes)

See the [install instructions](README.md#install) for verification commands.
